pub mod deliver;
pub mod driver;

use crate::naming::PackageKind;
use crate::{Result, error::CliError, ui};
use driver::{BuildReport, Profile, ReportedArtifact, Selected};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct BuildOptions {
    pub release: bool,
    pub target: Option<String>,
    pub output: Option<String>,
    pub schema: Option<String>,
    pub no_api: bool,
    pub gen_config: forgedb_codegen::GenConfig,
    pub config_targets: Vec<String>,
    pub cache_container: Option<PathBuf>,
    pub cache_project: Option<PathBuf>,
    pub in_tree: Option<PathBuf>,
    pub plan_only: bool,
    pub report: Option<String>,
    pub print_artifact: Option<String>,
    pub sync: Option<crate::commands::dev::SyncHook>,
}

pub fn stdout_is_machine_readable(print_artifact: Option<&str>, report: Option<&str>) -> bool {
    print_artifact.is_some() || report == Some("-")
}

impl BuildOptions {
    fn machine_flag_name(&self) -> Option<&'static str> {
        if self.report.is_some() {
            return Some("--report");
        }
        if self.print_artifact.is_some() {
            return Some("--print-artifact");
        }
        None
    }
}

pub fn run(options: BuildOptions) -> Result<()> {
    reject_retired_target_flag(options.target.as_deref())?;
    if options.plan_only
        && let Some(other) = options.machine_flag_name()
    {
        return Err(CliError::Build(format!(
            "`--plan` and `{other}` cannot be combined: `--plan` compiles nothing, so there \
             are no artifacts to report, and an empty report — or a silently skipped path — \
             from a command that exited 0 is the reads-as-applied failure this design removes \
             everywhere else."
        )));
    }

    ui::header("🔨", "Building production artifacts");

    ui::info("Validating schema...");
    crate::commands::validate::run(crate::commands::validate::ValidateOptions {
        strict: false,
        schema_only: true,
        implementations: false,
        components: false,
        schema: options.schema.clone(),
    })?;

    let selected_targets = apply_no_api(&options.config_targets, options.no_api);

    ui::info("Generating code...");
    if options.no_api {
        ui::info("Skipping API generation (--no-api)");
    }
    crate::commands::generate::run(crate::commands::generate::GenerateOptions {
        target: "all".to_string(),
        mode: None,
        check: false,
        output: options.output.clone(),
        schema: options.schema.clone(),
        config_targets: Some(selected_targets.clone()),
        cache_container: options.cache_container.clone(),
        in_tree: options.in_tree.clone(),
        gen_config: options.gen_config,
        force: true,
        from: None,
        to: None,
    })?;

    if let Some(sync) = &options.sync {
        sync()?;
    }

    let Some(container) = options.cache_container.as_deref() else {
        return Err(CliError::Build(
            "no build cache was reserved for this app, so there is nothing to compile. \
             This is a ForgeDB bug; please report it."
                .to_string(),
        ));
    };
    let Some(project_root) = options.cache_project.as_deref() else {
        return Err(CliError::Build(
            "no build cache workspace root was reserved for this app. \
             This is a ForgeDB bug; please report it."
                .to_string(),
        ));
    };

    let selected = select_packages(
        container,
        options.schema.as_deref().unwrap_or_default(),
        &crate::targets::declared_packages(&selected_targets),
    )?;

    let invocations = driver::plan(project_root, &selected, options.release);

    if options.plan_only {
        print_plan(&selected, &invocations);
        return Ok(());
    }

    if selected.is_empty() {
        ui::info("No cargo packages to build for this target set.");
    } else {
        driver::assert_no_duplicate_artifact_names(project_root)?;
    }

    let artifacts = driver::execute(&invocations)?;

    let report = build_report(
        project_root,
        container,
        options.release,
        &selected,
        &invocations,
        &artifacts,
    )?;

    ui::success("Build complete!");
    for artifact in &report.artifacts {
        ui::info(&format!(
            "{} ({}): {}",
            artifact.kind,
            artifact.target_kind.as_str(),
            artifact.path.display()
        ));
    }

    let mut report = report;
    report.delivered = deliver::run_if_output(options.output.as_deref(), &report)?;

    if let Some(dest) = &options.report {
        emit_report(&report, dest)?;
    }
    if let Some(kind) = &options.print_artifact {
        println!("{}", report.print_artifact(kind)?.display());
    }

    Ok(())
}

fn reject_retired_target_flag(target: Option<&str>) -> Result<()> {
    let Some(target) = target else { return Ok(()) };
    Err(CliError::Build(format!(
        "`--target {target}` was removed.\n\n\
         What ForgeDB builds is decided by `[generate].targets` in `forgedb.toml`, not by a \
         flag: a cargo `--target` is invocation-wide, so it could never mean \"also build the \
         browser replica\" — it meant \"build EVERYTHING for this triple\", which the replica's \
         siblings cannot be.\n\n\
         To build the browser read-replica, declare it:\n\n\
         \x20   [generate]\n\
         \x20   targets = [\"rust\", \"browser-replica\"]\n\n\
         `forgedb build` then compiles it for `{}` in its own cargo invocation, automatically. \
         Run `forgedb build --plan` to see exactly what it will run.",
        driver::WASM_TRIPLE
    )))
}

fn apply_no_api(config_targets: &[String], no_api: bool) -> Vec<String> {
    if !no_api {
        return config_targets.to_vec();
    }
    config_targets
        .iter()
        .filter(|t| t.as_str() != "api" && t.as_str() != "openapi")
        .cloned()
        .collect()
}

fn select_packages(
    container: &Path,
    schema: &str,
    declared: &[PackageKind],
) -> Result<Vec<Selected>> {
    let app_name = crate::cache::member_app_name(container).unwrap_or_else(|| {
        crate::naming::app_name(
            "",
            Path::new(schema),
            &[],
            crate::naming::SymbolNaming::Minimal,
        )
    });

    let mut found: BTreeMap<String, Selected> = BTreeMap::new();
    let entries = match std::fs::read_dir(container) {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("Cargo.toml").is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(kind) = PackageKind::from_dir(&name) else {
            continue;
        };
        if kind.owner() != crate::naming::PruneOwner::GenerateBuild {
            continue;
        }
        if !declared.contains(&kind) {
            continue;
        }
        let package = crate::naming::package_name(&app_name, &kind);
        found.insert(kind.dir(), Selected { package, kind });
    }

    Ok(found.into_values().collect())
}

fn print_plan(selected: &[Selected], invocations: &[driver::Invocation]) {
    println!();
    println!("Packages:");
    if selected.is_empty() {
        println!("  (none — this target set declares no cargo package)");
    }
    for s in selected {
        println!("  {:<18} {}", s.kind.dir(), s.package);
    }
    println!();
    println!("Invocations:");
    if invocations.is_empty() {
        println!("  (none)");
    }
    for inv in invocations {
        println!("  cd {}", inv.cwd.display());
        println!("  {}", inv.command_line());
        println!();
    }
    println!("Nothing was compiled (--plan).");
}

fn build_report(
    project_root: &Path,
    container: &Path,
    release: bool,
    selected: &[Selected],
    invocations: &[driver::Invocation],
    artifacts: &[driver::Artifact],
) -> Result<BuildReport> {
    let mut kinds: BTreeMap<&str, &PackageKind> = BTreeMap::new();
    for s in selected {
        kinds.insert(s.package.as_str(), &s.kind);
    }

    let mut host: Option<String> = None;
    let mut triples: BTreeMap<String, String> = BTreeMap::new();
    for inv in invocations {
        let triple = match inv.triple() {
            Some(t) => t.to_string(),
            None => match &host {
                Some(h) => h.clone(),
                None => {
                    let h = driver::host_triple()?;
                    host = Some(h.clone());
                    h
                }
            },
        };
        for package in inv.packages() {
            triples.insert(package, triple.clone());
        }
    }

    let mut reported = Vec::new();
    for artifact in artifacts {
        let Some(kind) = kinds.get(artifact.package.as_str()) else {
            continue;
        };
        let triple = triples
            .get(&artifact.package)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        reported.push(ReportedArtifact {
            package: artifact.package.clone(),
            kind: kind.dir(),
            target_kind: artifact.kind,
            path: artifact.path.clone(),
            triple,
        });
    }

    Ok(BuildReport {
        version: driver::REPORT_VERSION,
        project: project_root.to_path_buf(),
        app: container.to_path_buf(),
        profile: if release {
            Profile::Release
        } else {
            Profile::Debug
        },
        artifacts: reported,
        delivered: Vec::new(),
    })
}

fn emit_report(report: &BuildReport, dest: &str) -> Result<()> {
    let json = report.to_json()?;
    if dest == "-" {
        println!("{json}");
        return Ok(());
    }
    let path = PathBuf::from(dest);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, format!("{json}\n"))?;
    ui::info(&format!("Artifact report: {}", path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retired_target_flag_names_its_replacement() {
        assert!(reject_retired_target_flag(None).is_ok());
        let err = reject_retired_target_flag(Some("wasm"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("was removed"), "{err}");
        assert!(
            err.contains("[generate]") && err.contains("browser-replica"),
            "the error must name the config key that replaced the flag: {err}"
        );
    }

    #[test]
    fn no_api_drops_the_api_and_openapi_targets_only() {
        let declared: Vec<String> = ["rust", "api", "openapi", "ffi", "typescript"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(apply_no_api(&declared, false), declared);
        assert_eq!(
            apply_no_api(&declared, true),
            vec![
                "rust".to_string(),
                "ffi".to_string(),
                "typescript".to_string()
            ]
        );
    }
}
