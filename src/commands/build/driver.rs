use crate::error::{CliError, Result};
use crate::naming::PackageKind;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const WASM_TRIPLE: &str = "wasm32-unknown-unknown";

const MESSAGE_FORMAT: &str = "--message-format=json-render-diagnostics";

const COLLISION_MARKER: &str = "output filename collision";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub package: String,
    pub kind: PackageKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl Invocation {
    pub fn packages(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut want = false;
        for arg in &self.args {
            if want {
                out.push(arg.clone());
                want = false;
                continue;
            }
            if arg == "-p" || arg == "--package" {
                want = true;
            } else if let Some(rest) = arg.strip_prefix("--package=") {
                out.push(rest.to_string());
            }
        }
        out
    }

    pub fn triple(&self) -> Option<&str> {
        let mut want = false;
        for arg in &self.args {
            if want {
                return Some(arg.as_str());
            }
            if arg == "--target" {
                want = true;
            } else if let Some(rest) = arg.strip_prefix("--target=") {
                return Some(rest);
            }
        }
        None
    }

    pub fn command_line(&self) -> String {
        let mut out = String::from("cargo");
        for arg in &self.args {
            out.push(' ');
            out.push_str(&shell_quote(arg));
        }
        out
    }
}

fn shell_quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '=' | ':' | '+')
        });
    if safe {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

pub fn plan(root: &Path, selected: &[Selected], release: bool) -> Vec<Invocation> {
    let manifest = root.join("Cargo.toml");
    let profile = if release { "release" } else { "dev" };

    let (wasm, native): (Vec<&Selected>, Vec<&Selected>) = selected
        .iter()
        .partition(|s| matches!(s.kind, PackageKind::Wasm));

    let base = |extra: &[String], packages: &[&Selected]| -> Invocation {
        let mut args: Vec<String> = vec![
            "build".to_string(),
            "--manifest-path".to_string(),
            manifest.display().to_string(),
            MESSAGE_FORMAT.to_string(),
        ];
        if release {
            args.push("--release".to_string());
        }
        args.extend(extra.iter().cloned());
        for s in packages {
            args.push("-p".to_string());
            args.push(s.package.clone());
        }
        Invocation {
            args,
            cwd: root.to_path_buf(),
            env: Vec::new(),
        }
    };

    let mut out = Vec::new();
    if !native.is_empty() {
        out.push(base(
            &[
                "--config".to_string(),
                format!("profile.{profile}.panic=\"unwind\""),
            ],
            &native,
        ));
    }
    if !wasm.is_empty() {
        out.push(base(
            &[
                "--target".to_string(),
                WASM_TRIPLE.to_string(),
                "--config".to_string(),
                format!("profile.{profile}.opt-level=\"s\""),
            ],
            &wasm,
        ));
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Bin,
    Cdylib,
    Staticlib,
    Rlib,
}

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Bin => "bin",
            TargetKind::Cdylib => "cdylib",
            TargetKind::Staticlib => "staticlib",
            TargetKind::Rlib => "rlib",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Artifact {
    pub package: String,
    pub kind: TargetKind,
    pub path: PathBuf,
}

fn kind_from_filename(path: &Path) -> Option<TargetKind> {
    match path.extension()?.to_str()? {
        "rlib" => Some(TargetKind::Rlib),
        "a" | "lib" => Some(TargetKind::Staticlib),
        "so" | "dylib" | "dll" | "wasm" => Some(TargetKind::Cdylib),
        _ => None,
    }
}

fn package_name_from_id(id: &str) -> Option<String> {
    let Some((url, fragment)) = id.rsplit_once('#') else {
        let name = id.split_whitespace().next()?;
        return (!name.is_empty()).then(|| name.to_string());
    };

    if let Some((name, _version)) = fragment.rsplit_once('@') {
        return (!name.is_empty()).then(|| name.to_string());
    }
    if fragment.starts_with(|c: char| c.is_ascii_digit()) {
        let last = url.rsplit(['/', '\\']).find(|s| !s.is_empty())?;
        return Some(last.to_string());
    }
    (!fragment.is_empty()).then(|| fragment.to_string())
}

pub fn parse_artifacts(stdout: &str) -> Vec<Artifact> {
    let mut out: Vec<Artifact> = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let Some(package) = msg
            .get("package_id")
            .and_then(|v| v.as_str())
            .and_then(package_name_from_id)
        else {
            continue;
        };

        let kinds: Vec<&str> = msg
            .pointer("/target/kind")
            .and_then(|k| k.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        if kinds
            .iter()
            .any(|k| matches!(*k, "custom-build" | "test" | "bench" | "example"))
        {
            continue;
        }

        if let Some(exe) = msg.get("executable").and_then(|e| e.as_str())
            && !exe.is_empty()
        {
            out.push(Artifact {
                package: package.clone(),
                kind: TargetKind::Bin,
                path: PathBuf::from(exe),
            });
        }

        for value in msg
            .get("filenames")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
        {
            let Some(name) = value.as_str() else { continue };
            let path = PathBuf::from(name);
            let Some(kind) = kind_from_filename(&path) else {
                continue;
            };
            out.push(Artifact {
                package: package.clone(),
                kind,
                path,
            });
        }
    }

    out.sort();
    out.dedup();
    out
}

fn collision_class(kind: &str) -> Option<&'static str> {
    match kind {
        "bin" => Some("bin"),
        "cdylib" | "dylib" => Some("dylib"),
        "staticlib" => Some("staticlib"),
        _ => None,
    }
}

pub fn duplicate_artifact_names(metadata: &serde_json::Value) -> Option<String> {
    let mut seen: BTreeMap<(&'static str, String), BTreeSet<String>> = BTreeMap::new();

    for pkg in metadata
        .get("packages")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
    {
        let Some(pkg_name) = pkg.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        for target in pkg
            .get("targets")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
        {
            let Some(target_name) = target.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            for kind in target
                .get("kind")
                .and_then(|k| k.as_array())
                .into_iter()
                .flatten()
                .filter_map(|k| k.as_str())
            {
                let Some(class) = collision_class(kind) else {
                    continue;
                };
                seen.entry((class, target_name.to_string()))
                    .or_default()
                    .insert(pkg_name.to_string());
            }
        }
    }

    let mut report = String::new();
    for ((class, name), packages) in &seen {
        if packages.len() < 2 {
            continue;
        }
        report.push_str(&format!(
            "two packages in the build cache would write the same `{class}` artifact `{name}`:\n"
        ));
        for p in packages {
            report.push_str(&format!("  - {p}\n"));
        }
    }

    if report.is_empty() {
        return None;
    }
    report.push_str(
        "\nCargo reports this as `warning: output filename collision`, exits 0, and leaves \
         exactly ONE of the two files behind — so a later command that resolves a binary by \
         name can run the wrong one over a user's data directory at exit 0. \
         (`cargo check` never links, so it cannot see this condition at all.)\n\
         This is a ForgeDB bug: report it with the two package names above.",
    );
    Some(report)
}

fn metadata(root: &Path) -> Result<serde_json::Value> {
    let manifest = root.join("Cargo.toml");
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .arg("--manifest-path")
        .arg(&manifest)
        .current_dir(root)
        .output()
        .map_err(|e| CliError::Build(format!("failed to run `cargo metadata`: {e}")))?;

    if !out.status.success() {
        return Err(CliError::Build(format!(
            "`cargo metadata` failed for the build cache at {}:\n{}",
            root.display(),
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    serde_json::from_slice(&out.stdout)
        .map_err(|e| CliError::Build(format!("`cargo metadata` emitted unreadable JSON: {e}")))
}

pub fn target_directory(root: &Path) -> Result<PathBuf> {
    let meta = metadata(root)?;
    meta.get("target_directory")
        .and_then(|t| t.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Build(format!(
                "`cargo metadata` reported no target directory for the build cache at {}",
                root.display()
            ))
        })
}

pub fn assert_no_duplicate_artifact_names(root: &Path) -> Result<()> {
    let metadata = metadata(root)?;

    match duplicate_artifact_names(&metadata) {
        Some(message) => Err(CliError::Build(message)),
        None => Ok(()),
    }
}

pub fn execute(invocations: &[Invocation]) -> Result<Vec<Artifact>> {
    let mut all = Vec::new();

    for inv in invocations {
        let wanted: BTreeSet<String> = inv.packages().into_iter().collect();

        let mut cmd = Command::new("cargo");
        cmd.args(&inv.args)
            .current_dir(&inv.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &inv.env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| CliError::Build(format!("failed to run cargo: {e}")))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CliError::Build("cargo stderr was not captured".to_string()))?;
        let pump = std::thread::spawn(move || -> String {
            let mut reader = BufReader::new(stderr);
            let mut seen = String::new();
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        eprint!("{line}");
                        seen.push_str(&line);
                    }
                }
            }
            seen
        });

        let mut stdout = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            let mut raw = Vec::new();
            pipe.read_to_end(&mut raw)
                .map_err(|e| CliError::Build(format!("failed to read cargo output: {e}")))?;
            stdout = String::from_utf8_lossy(&raw).into_owned();
        }

        let status = child
            .wait()
            .map_err(|e| CliError::Build(format!("failed to wait for cargo: {e}")))?;
        let stderr_text = pump.join().unwrap_or_default();

        if !status.success() {
            return Err(CliError::Build(
                "the build failed (see the cargo output above)".to_string(),
            ));
        }

        if stderr_text.contains(COLLISION_MARKER) {
            return Err(CliError::Build(format!(
                "cargo reported an output filename collision and exited 0, so one of the \
                 colliding files has been silently overwritten. The build cache at {} is not \
                 trustworthy — this is a ForgeDB bug; please report it.\n\nCargo said:\n{}",
                inv.cwd.display(),
                stderr_text
                    .lines()
                    .filter(|l| l.contains(COLLISION_MARKER))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }

        for artifact in parse_artifacts(&stdout) {
            if !wanted.contains(&artifact.package) {
                continue;
            }
            if !artifact.path.is_file() {
                return Err(CliError::Build(format!(
                    "cargo reported `{}` at {} but nothing is there",
                    artifact.package,
                    artifact.path.display()
                )));
            }
            all.push(artifact);
        }
    }

    all.sort();
    all.dedup();
    Ok(all)
}

pub fn host_triple() -> Result<String> {
    let out = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| CliError::Build(format!("failed to run `rustc -vV`: {e}")))?;
    if !out.status.success() {
        return Err(CliError::Build(
            "`rustc -vV` failed; cannot determine the host triple".to_string(),
        ));
    }
    parse_host_triple(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| CliError::Build("`rustc -vV` printed no `host:` line".to_string()))
}

pub fn parse_host_triple(vv: &str) -> Option<String> {
    vv.lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(|t| t.trim().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Debug,
    Release,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportedArtifact {
    pub package: String,
    pub kind: String,
    pub target_kind: TargetKind,
    pub path: PathBuf,
    pub triple: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeliveredArtifact {
    pub kind: String,
    pub target_kind: TargetKind,
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug, serde::Serialize)]
pub struct BuildReport {
    pub version: u32,
    pub project: PathBuf,
    pub app: PathBuf,
    pub profile: Profile,
    pub artifacts: Vec<ReportedArtifact>,
    #[serde(default)]
    pub delivered: Vec<DeliveredArtifact>,
}

pub const REPORT_VERSION: u32 = 1;

pub fn primary_target_kind(kind: &PackageKind) -> TargetKind {
    match kind {
        PackageKind::Core => TargetKind::Rlib,
        PackageKind::Server | PackageKind::Transform { .. } | PackageKind::Engine { .. } => {
            TargetKind::Bin
        }
        PackageKind::Ffi => TargetKind::Staticlib,
        PackageKind::Napi | PackageKind::Pyo3 | PackageKind::Wasm => TargetKind::Cdylib,
    }
}

impl BuildReport {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| CliError::Build(format!("could not render the build report: {e}")))
    }

    pub fn print_artifact(&self, kind: &str) -> Result<&Path> {
        let Some(package_kind) = PackageKind::from_dir(kind) else {
            return Err(CliError::Build(format!(
                "`--print-artifact {kind}`: `{kind}` is not a ForgeDB package kind.\n\
                 Legal kinds: core, server, napi, pyo3, ffi, wasm, transform-<from>-<to>, \
                 engine-<from>-<to>.\n\
                 (It is a KIND, never a package name: package names are derived from the app's \
                 path, so a baked one breaks when the schema file is moved or renamed.)"
            )));
        };
        let want = primary_target_kind(&package_kind);

        let hits: Vec<&ReportedArtifact> = self
            .artifacts
            .iter()
            .filter(|a| a.kind == kind && a.target_kind == want)
            .collect();

        match hits.as_slice() {
            [one] => Ok(one.path.as_path()),
            [] => Err(CliError::Build(format!(
                "`--print-artifact {kind}` matched nothing: this build produced no `{}` \
                 artifact for a `{kind}` package.\n\nIt produced:\n{}",
                want.as_str(),
                self.render_inventory()
            ))),
            many => Err(CliError::Build(format!(
                "`--print-artifact {kind}` is ambiguous — {} artifacts match:\n{}",
                many.len(),
                many.iter()
                    .map(|a| format!("  {} ({})", a.path.display(), a.package))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))),
        }
    }

    pub(crate) fn render_inventory(&self) -> String {
        if self.artifacts.is_empty() {
            return "  (nothing)".to_string();
        }
        self.artifacts
            .iter()
            .map(|a| {
                format!(
                    "  {:<10} {:<10} {}",
                    a.kind,
                    a.target_kind.as_str(),
                    a.path.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(package: &str, kind: PackageKind) -> Selected {
        Selected {
            package: package.to_string(),
            kind,
        }
    }

    #[test]
    fn the_target_split_is_forced_not_chosen() {
        let root = Path::new("/cache/proj");
        let plan = plan(
            root,
            &[
                sel("a-core", PackageKind::Core),
                sel("a-wasm", PackageKind::Wasm),
                sel("a-server", PackageKind::Server),
            ],
            true,
        );
        assert_eq!(plan.len(), 2, "wasm must be its own invocation: {plan:#?}");
        assert_eq!(plan[0].triple(), None);
        assert_eq!(plan[1].triple(), Some(WASM_TRIPLE));
        assert_eq!(plan[0].packages(), vec!["a-core", "a-server"]);
        assert_eq!(plan[1].packages(), vec!["a-wasm"]);
    }

    #[test]
    fn a_rust_only_app_plans_one_invocation() {
        let plan = plan(Path::new("/c"), &[sel("a-core", PackageKind::Core)], false);
        assert_eq!(plan.len(), 1);
        assert!(!plan[0].args.iter().any(|a| a == "--release"));
        assert!(
            plan[0]
                .args
                .contains(&"profile.dev.panic=\"unwind\"".to_string())
        );
    }

    #[test]
    fn an_empty_selection_plans_nothing() {
        assert!(plan(Path::new("/c"), &[], true).is_empty());
    }

    #[test]
    fn package_ids_are_read_in_all_three_spellings() {
        assert_eq!(
            package_name_from_id("path+file:///a/b/core#blog-3f2a-core@0.1.0").as_deref(),
            Some("blog-3f2a-core")
        );
        assert_eq!(
            package_name_from_id("path+file:///a/b/blog-3f2a-core#0.1.0").as_deref(),
            Some("blog-3f2a-core")
        );
        assert_eq!(
            package_name_from_id(
                "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0"
            )
            .as_deref(),
            Some("serde")
        );
        assert_eq!(
            package_name_from_id("blog-3f2a-core 0.1.0 (path+file:///a/b)").as_deref(),
            Some("blog-3f2a-core")
        );
    }

    #[test]
    fn parse_host_triple_reads_the_host_line() {
        let vv = "rustc 1.96.0 (abc 2026-01-01)\nbinary: rustc\nhost: aarch64-apple-darwin\nrelease: 1.96.0\n";
        assert_eq!(
            parse_host_triple(vv).as_deref(),
            Some("aarch64-apple-darwin")
        );
        assert_eq!(parse_host_triple("no host line here"), None);
    }

    #[test]
    fn shell_quoting_survives_the_profile_floor() {
        let q = shell_quote("profile.release.panic=\"unwind\"");
        assert!(q.starts_with('\'') && q.ends_with('\''), "{q}");
    }
}
