//! `forgedb build` — generate, then compile the cache workspace (#335 step 6).
//!
//! # What this command used to do
//!
//! It ran `cargo build` **in the user's working directory**, with no
//! `--manifest-path` and no package selection.  In a directory holding an
//! unrelated crate that compiled *that* crate, printed `✓ Compiled database
//! (native)` and exited 0 — reproduced end to end.  It also probed `rustup`,
//! installed a target behind the user's back, and finished by printing an
//! `Artifacts:` block naming a directory it had never looked at.
//!
//! Everything it compiles now lives in the ForgeDB-owned cache workspace, is
//! named explicitly with `-p`, and is reported only after the file has been
//! found on disk.  The mechanics are in [`driver`]; this module decides *what*
//! to build and turns the result into the two machine-readable surfaces
//! (`--report`, `--print-artifact`) that #335 §14/§15 owes the deploy path.

pub mod deliver;
pub mod driver;

use crate::naming::PackageKind;
use crate::{Result, error::CliError, ui};
use driver::{BuildReport, Profile, ReportedArtifact, Selected};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct BuildOptions {
    pub release: bool,
    /// The retired `--target` flag.
    ///
    /// Kept as an option purely so passing it is an **error that names its
    /// replacement**, rather than clap's `unexpected argument`.  A flag that
    /// reads as applied and is not is the failure mode this issue deletes
    /// everywhere else, and `--target wasm` was exactly that: it selected a
    /// whole-invocation rust target, which is not what "build the browser
    /// replica" means.
    pub target: Option<String>,
    pub output: Option<String>,
    pub schema: Option<String>,
    pub no_api: bool,
    /// Generate-time runtime-behavior config (epic #126), resolved by the caller
    /// from the **one** config the CLI loaded.
    ///
    /// This is passed in rather than loaded here (#361): `build` used to call
    /// the config itself, so a single invocation was served by two
    /// different files — `--config` for the output/schema paths and whatever sat
    /// in the working directory for every `[runtime]`/`[storage]` knob. Taking it
    /// as an argument is what makes that unrepresentable; two loaders that merely
    /// agree today is the same bug waiting.
    pub gen_config: forgedb_codegen::GenConfig,
    /// The project's declared target set, as canonical internal names (#335 §12).
    ///
    /// This used to be hardcoded `None` at both call sites below, which made
    /// every opt-in arm of `generate_all` — `ffi`, the browser replica, the
    /// three REST SDKs, and (once they existed) the three native bindings —
    /// unreachable from `forgedb build`. `build` could not reach a target the
    /// project had explicitly declared.
    pub config_targets: Vec<String>,
    /// The app's container in the build cache, reserved by the caller.
    pub cache_container: Option<PathBuf>,
    /// The cache **workspace root** — `~/.forgedb/projects/<id>` — reserved by
    /// the caller.
    ///
    /// Handed in rather than re-derived from `cache_container`: `cache::reserve`
    /// already computed both, and a second derivation is a second thing that can
    /// disagree.
    pub cache_project: Option<PathBuf>,
    /// Where the in-tree Rust package goes (#338), or `None` when the knob is
    /// absent.
    ///
    /// `build` regenerates before it plans, so it must carry this or a project
    /// with the knob set would have `generate` and `build` emit different
    /// project states — the exact class of defect #364 was. `build` does **not**
    /// compile it: class D has no delivery step, and `driver::plan` never sees
    /// this path.
    pub in_tree: Option<PathBuf>,
    /// Print the cargo invocations and compile nothing.
    pub plan_only: bool,
    /// Write the JSON artifact report here; `-` means stdout.
    pub report: Option<String>,
    /// Print exactly one absolute artifact path for this package **kind**.
    pub print_artifact: Option<String>,
    /// Re-derive the cache workspace root, **after** generation and **before**
    /// cargo (#335 §3).
    ///
    /// It has to run in the middle of this function, which is why it is handed
    /// in rather than left to the caller: the root manifest is rendered from a
    /// scan of what is on disk, and `build` emits the packages it is about to
    /// compile. A caller that synced only after `build::run` returned would hand
    /// cargo the *previous* run's member list — so an app's very first
    /// `forgedb build` would fail with `error: package(s) … not found`, and only
    /// on a cold cache.
    ///
    /// Typed as [`crate::commands::dev::SyncHook`] rather than as a second
    /// identical alias, so the de-list/prune/render order keeps exactly one
    /// definition.
    pub sync: Option<crate::commands::dev::SyncHook>,
}

/// Whether an invocation with these two flags reserves stdout for a machine.
///
/// **A free function taking the flags, not a method on [`BuildOptions`], because
/// the only caller cannot have a `BuildOptions` yet.** `main.rs` must silence
/// the human output *before* `project::identify_reported` and
/// `reserve_in_cache` print — and those run while it is still assembling the
/// value. A method here would therefore have been unreachable from the one place
/// that needs it, and the condition would live inline in `main.rs` as a second
/// copy: two definitions of "is stdout spoken for", drifting apart the moment a
/// third machine-readable flag is added.
///
/// `--report <file>` is deliberately *not* machine-readable in this sense: the
/// document goes to the file, so stdout stays the human's. Only `--report -`
/// hands it over.
pub fn stdout_is_machine_readable(print_artifact: Option<&str>, report: Option<&str>) -> bool {
    print_artifact.is_some() || report == Some("-")
}

impl BuildOptions {
    /// The name of whichever artifact-consuming flag was passed, for the
    /// `--plan` conflict diagnostic.
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

    // First, validate the schema
    ui::info("Validating schema...");
    crate::commands::validate::run(crate::commands::validate::ValidateOptions {
        strict: false,
        schema_only: true,
        implementations: false,
        components: false,
        schema: options.schema.clone(),
    })?;

    // `--no-api` is a PACKAGE SELECTION, not a different generation.
    //
    // It used to run `generate::run` three times over a hardcoded
    // `rust`/`node --sdk`/`stubs` list, which silently overrode the project's
    // declared target set: an app declaring `ffi` got no FFI from `build
    // --no-api`, and nothing said so. Narrowing the declared set instead keeps
    // one emission path and makes the flag mean exactly what it says.
    let selected_targets = apply_no_api(&options.config_targets, options.no_api);

    ui::info("Generating code...");
    if options.no_api {
        ui::info("Skipping API generation (--no-api)");
    }
    // Build always regenerates derived artifacts — pass force: true so a
    // second consecutive `build` does not fail on "File exists".
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

    // The root manifest names the members cargo may build, and it is rendered
    // from a scan — so it has to be re-derived HERE, between the emission above
    // and the compile below. See `BuildOptions::sync`.
    if let Some(sync) = &options.sync {
        sync()?;
    }

    let Some(container) = options.cache_container.as_deref() else {
        // Unreachable from the CLI — `main.rs` reserves a container before it
        // gets here — but a `None` must not silently mean "build nothing".
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
        // Not an error: a project declaring only `node-sdk`/`openapi`/`stubs`
        // has real output and no cargo package at all. Saying so is the point —
        // the old code ran `cargo build` unconditionally and reported success
        // for whatever it happened to compile.
        ui::info("No cargo packages to build for this target set.");
    } else {
        // Before the compile, because the condition it catches is one cargo
        // declines to treat as an error (it warns and exits 0), and because
        // `cargo check` never links and so cannot see it at all.
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

    // Delivery (#337). Every class-B target's compiled half is copied out of the
    // cache and into the app's `output`, beside the generated text that
    // describes it, and every delivered path is printed (C7).
    //
    // Go used to be the ONE carve-out from #335's "no delivery" non-goal,
    // because the generated cgo preamble says `#cgo LDFLAGS: -L${SRCDIR}
    // -lforgedb` — the Go package's *source* is already written against a
    // library sitting beside it. That row is now one row of the table, and its
    // destination is unchanged.
    //
    // It runs BEFORE the report is written, so `--report` names what was
    // delivered rather than describing a delivery that had not happened yet.
    let mut report = report;
    report.delivered = deliver::run_if_output(options.output.as_deref(), &report)?;

    // Both machine surfaces are projections of the SAME value, written only
    // after every path in it has been existence-checked by `driver::execute`.
    if let Some(dest) = &options.report {
        emit_report(&report, dest)?;
    }
    if let Some(kind) = &options.print_artifact {
        println!("{}", report.print_artifact(kind)?.display());
    }

    Ok(())
}

/// The removed `--target` flag, with the thing that replaced it.
///
/// `--target` was invocation-wide and meant a *rust* target triple, so
/// `--target wasm` never built the browser replica: it asked cargo to build the
/// whole selected package set for a triple it then failed to resolve. What
/// decides whether the replica is built is the project's declared target set.
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

/// `--no-api` as a narrowing of the declared target set.
///
/// `openapi` goes with `api`: the spec describes the server this flag is
/// declining to build, and emitting a document for an artifact that does not
/// exist is the same class of lie as the old unchecked `Artifacts:` block.
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

/// Which cache packages this build compiles.
///
/// The set is the **intersection** of what the config declares and what is
/// actually on disk, and both halves matter:
///
/// * declared-but-absent would put a `-p <name>` on the command line for a
///   package cargo has never heard of — `error: package(s) … not found`, a hard
///   failure for a target the user merely declared;
/// * present-but-undeclared is `migrate`'s `transform-*`/`engine-*`, which
///   `build` must not compile (they are built on demand, for one lineage range)
///   and must not prune either.  [`PackageKind::owner`] is what draws that line,
///   and it is drawn here rather than by a name pattern.
fn select_packages(
    container: &Path,
    schema: &str,
    declared: &[PackageKind],
) -> Result<Vec<Selected>> {
    // Read the app's derived name from the container rather than re-deriving
    // it: `naming::app_name` needs the project's whole app set, and a second
    // derivation that disagreed would select packages cargo has never heard of.
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
        // A container that does not exist yet means nothing was emitted; that
        // is the empty selection, not a failure.
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

/// `forgedb build --plan`.
///
/// This is [`driver::plan`]'s user-facing surface, and printing it is what keeps
/// the plan honest: the profile floor, the manifest path and the package set are
/// all visible here, so a change to any of them is a change to something a user
/// can read. Written with `println!` rather than `ui::info` because it is the
/// command's **output**, not its commentary.
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

/// Turn the raw artifact list into the reportable shape (#335 §14).
///
/// The `triple` comes from the **invocation that selected the package**, not
/// from the artifact's path: the wasm arm is a separate cargo process with a
/// different triple, and reading a triple back out of a path is a guess about a
/// directory layout cargo owns.
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

    // Resolved lazily: `rustc -vV` must not run for a build that produced no
    // native artifact (and must not run at all under `--plan`, which never
    // reaches this function).
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
        // Filled by `deliver::run` after the compile; empty here because nothing
        // has been delivered yet, and a report that claimed otherwise would be
        // describing the future.
        delivered: Vec::new(),
    })
}

/// Write `--report`.
///
/// `-` means stdout, and in that mode every human-facing line has already been
/// silenced by the caller so the document is parseable with no filter.
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
