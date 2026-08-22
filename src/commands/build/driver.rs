//! The generalised cargo driver (#335 §8, plan #347 step 6).
//!
//! # Why this is a module and not three lines inside `run`
//!
//! Everything that can go wrong in "ForgeDB owns the build" lives at the process
//! boundary, and none of it is visible to a string snapshot: which manifest cargo
//! chose, which packages it was told to build, which profile settings actually
//! applied, and where the files it produced really are.  So the boundary is split
//! into two **pure** functions and one impure one:
//!
//! * [`plan`] builds the exact argument vectors.  Pure, and printed verbatim by
//!   `forgedb build --plan` — a plan nobody can print drifts from the plan that
//!   runs.
//! * [`parse_artifacts`] turns cargo's own JSON stream into [`Artifact`]s.  Pure,
//!   and tested by **replaying a recorded stream**, never by mocking cargo.
//! * [`execute`] is the only part that spawns anything.
//!
//! # Cargo is never mocked
//!
//! The defect this issue fixes is a misunderstanding of what cargo does with the
//! working directory.  A mock would encode the same misunderstanding and go
//! green (`docs/` and CLAUDE.md both carry this rule).  Where a real build is too
//! expensive the seam is `--plan` and a recorded artifact stream — not a fake
//! cargo.
//!
//! # The four measured hazards this module exists to defeat
//!
//! 1. **Cargo's `config.toml` beats the manifest.** A machine-wide
//!    `$CARGO_HOME/config.toml` setting `profile.release.panic = "abort"` breaks
//!    the FFI `catch_unwind` boundary in the generated `ffi`/`napi` wrappers —
//!    measured: the same manifest that catches its unwind and exits 0 instead
//!    dies `Abort trap: 6` (exit 134) with `catch_unwind` never firing.  The
//!    floor is therefore applied as a **driver `--config` argument**, which beats
//!    every config file, and it is an argument rather than
//!    `CARGO_PROFILE_RELEASE_PANIC` **because an argument is visible** in what
//!    `--plan` prints.  An env var is the invisible mechanism the hazard is made
//!    of.
//! 2. **`[profile.*]` in a workspace member is silently ignored**
//!    (`warning: profiles for the non root package will be ignored`), so the
//!    wrappers cannot defend themselves; only the root or the command line can.
//! 3. **`--target` is invocation-wide.**  One cargo invocation cannot express a
//!    package set containing `wasm/`: cargo builds *every* selected package for
//!    *every* named target, and the wasm package then fails `E0432` against the
//!    host triple.  The split is on the **target** axis, forced — never on the
//!    package axis, which would forfeit the shared dependency graph the cache
//!    exists to provide.
//! 4. **`--current-dir` is not a cargo flag and `-C` is nightly-gated on the
//!    pinned 1.96.**  The working directory is set with [`std::process::Command::current_dir`]
//!    (what `migrate.rs` already does) *and* the manifest is named explicitly with
//!    `--manifest-path`, so the choice is visible in `--plan` rather than implied
//!    by an ambient cwd.

use crate::error::{CliError, Result};
use crate::naming::PackageKind;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The one triple the browser replica builds for.
pub const WASM_TRIPLE: &str = "wasm32-unknown-unknown";

/// The cargo message format the driver reads.
///
/// `json-render-diagnostics` puts one JSON message per line on **stdout** while
/// leaving human-readable diagnostics and progress on **stderr**, so capturing
/// stdout costs the user no output at all.
const MESSAGE_FORMAT: &str = "--message-format=json-render-diagnostics";

/// The substring cargo prints when two packages in one workspace would write the
/// same output file.  Cargo emits this as a **warning and exits 0**, leaving one
/// of the two files behind — see [`assert_no_duplicate_artifact_names`].
const COLLISION_MARKER: &str = "output filename collision";

// ---------------------------------------------------------------------------
// What to build
// ---------------------------------------------------------------------------

/// One cache package this invocation of `forgedb build` selected.
///
/// The `kind` is carried alongside the name because the target split ([`plan`])
/// and the artifact selector (`--print-artifact`) are both defined on the kind,
/// never on the package name: a package name is derived from the app's path and
/// changes when a schema moves, kinds do not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The cargo `[package] name` — `naming::package_name`, app name and all.
    pub package: String,
    /// Which ForgeDB package this is.
    pub kind: PackageKind,
}

/// One cargo process, fully described.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// Everything after the word `cargo`.
    pub args: Vec<String>,
    /// The working directory.  Always the cache workspace root — the whole point
    /// of #335 is that it is never the user's.
    pub cwd: PathBuf,
    /// Environment overrides.  Deliberately **empty** today: the profile floor is
    /// an argument so that `--plan` shows it (hazard 1 above), and nothing else
    /// needs one.  It exists so that a future override cannot be added as an
    /// invisible `Command::env` call at the spawn site.
    pub env: Vec<(String, String)>,
}

impl Invocation {
    /// The package names this invocation selected, read back out of its own
    /// `-p` arguments.
    ///
    /// Read back rather than carried so that [`Invocation`] stays the plan
    /// #347 shape and there is exactly one place the `-p` set is stated.
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

    /// The explicit `--target <triple>`, when this is the wasm arm.
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

    /// A copy-pasteable rendering, for `forgedb build --plan`.
    ///
    /// Shell-quoted, because the profile floor's value is TOML and contains
    /// double quotes: a plan a user cannot paste is a plan they will not check.
    pub fn command_line(&self) -> String {
        let mut out = String::from("cargo");
        for arg in &self.args {
            out.push(' ');
            out.push_str(&shell_quote(arg));
        }
        out
    }
}

/// Single-quote an argument unless it is plainly safe.
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

// ---------------------------------------------------------------------------
// plan() — PURE
// ---------------------------------------------------------------------------

/// The `-p` set, the target split, the profile floor, `--manifest-path`,
/// `--release`.  **Pure** — this touches no filesystem and spawns nothing.
///
/// At most two invocations come back, and which two is forced rather than
/// chosen (hazard 3 above):
///
/// | Arm | Packages | Extra |
/// |---|---|---|
/// | native | everything except [`PackageKind::Wasm`] | the `panic = "unwind"` floor |
/// | wasm | [`PackageKind::Wasm`] | `--target wasm32-unknown-unknown` + `opt-level = "s"` |
///
/// **The `panic` floor is native-only, on purpose.**  `panic = "unwind"` is what
/// keeps the `catch_unwind` boundary in the generated `ffi` and `napi` wrappers
/// real.  The browser replica has no such boundary — `catch_unwind` appears in
/// `crates/codegen/src/{ffi,napi}.rs` and in neither `wasm.rs` nor anything it
/// emits — so there is nothing there for a floor to protect, and forcing
/// unwinding would only add unwind tables to a bundle that has to travel over
/// the network.
///
/// Measured, not assumed: `wasm32-unknown-unknown` accepts **both** strategies
/// on the pinned toolchain (`cargo build --target wasm32-unknown-unknown
/// --config 'profile.release.panic="unwind"'` and `…="abort"` each exit 0), so
/// a floor here would be inert-but-costly rather than fatal.  An earlier draft
/// of this comment claimed the target pins `PanicStrategy::Abort` and that
/// `-C panic=unwind` is a hard rustc error there; that is false, and the
/// decision does not rest on it.
///
/// **The wasm `opt-level = "s"` came out of `crates/codegen/src/wasm.rs`**, where
/// it was a `[profile.release]` table in the replica's own manifest — i.e. a
/// profile in a workspace member, which cargo silently ignores (hazard 2).  It
/// reads as applied and is not, which is precisely the failure mode #335 deletes
/// everywhere else.
pub fn plan(root: &Path, selected: &[Selected], release: bool) -> Vec<Invocation> {
    let manifest = root.join("Cargo.toml");
    // `dev` is the profile NAME for a non-release build; `debug` is only the
    // directory it lands in. `--config profile.debug.…` is not a thing.
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

// ---------------------------------------------------------------------------
// parse_artifacts() — PURE
// ---------------------------------------------------------------------------

/// What kind of file cargo produced.
///
/// **Carried on [`Artifact`], never inferred by a consumer.**  Bins report
/// `executable`; lib targets report `filenames`, and one lib target reports
/// several: an rlib emits both `.rlib` and `.rmeta`, and the generated `ffi`
/// package declares `crate-type = ["cdylib", "rlib", "staticlib"]` and so emits
/// all three.  Every one of them exists on disk, so existence-checking cannot
/// discriminate — filtering by kind is the only way to hand Go delivery the
/// **staticlib** specifically.
///
/// The serde spelling is cargo's own `crate-type` spelling, so the report and the
/// manifest read alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Bin,
    Cdylib,
    Staticlib,
    Rlib,
}

impl TargetKind {
    /// The lowercase wire spelling, shared with `--print-artifact`'s diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Bin => "bin",
            TargetKind::Cdylib => "cdylib",
            TargetKind::Staticlib => "staticlib",
            TargetKind::Rlib => "rlib",
        }
    }
}

/// One file cargo produced, with the package that produced it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Artifact {
    pub package: String,
    pub kind: TargetKind,
    pub path: PathBuf,
}

/// Classify one of cargo's `filenames` entries by extension.
///
/// Extension rather than position, because cargo does **not** label the entries:
/// a `filenames` array is parallel to nothing.  `None` means "not a linkable
/// artifact" and covers two important cases — `.rmeta` (metadata, not a library)
/// and the extensionless copy of a bin, which arrives separately and
/// authoritatively as `executable`.
///
/// `.lib` on Windows is ambiguous (a staticlib *and* a cdylib's import library);
/// it is called a staticlib here, which is the reading Go delivery needs.  Both
/// of ForgeDB's supported hosts are unix, so this is a note rather than a
/// behaviour anyone reaches.
fn kind_from_filename(path: &Path) -> Option<TargetKind> {
    match path.extension()?.to_str()? {
        "rlib" => Some(TargetKind::Rlib),
        "a" | "lib" => Some(TargetKind::Staticlib),
        // `.wasm` is what a `crate-type = ["cdylib"]` produces for wasm32.
        "so" | "dylib" | "dll" | "wasm" => Some(TargetKind::Cdylib),
        _ => None,
    }
}

/// The package name inside a cargo `package_id`.
///
/// Cargo 1.77 replaced the old `name version (source)` spelling with a
/// `PackageIdSpec` URL, and the *name* is optional in it: cargo omits the
/// `#<name>@` part when the last path segment already equals the package name.
/// All three shapes are read here because a driver that understood only the one
/// its author happened to see would drop every artifact on the other.
///
/// ```text
/// path+file:///…/apps/3f2a/core#blog-3f2a-core@0.1.0   → blog-3f2a-core
/// path+file:///…/blog-3f2a-core#0.1.0                  → blog-3f2a-core
/// registry+https://…/index#serde@1.0.0                 → serde
/// blog-3f2a-core 0.1.0 (path+file:///…)                → blog-3f2a-core
/// ```
fn package_name_from_id(id: &str) -> Option<String> {
    let Some((url, fragment)) = id.rsplit_once('#') else {
        // Legacy `name version (source)`.
        let name = id.split_whitespace().next()?;
        return (!name.is_empty()).then(|| name.to_string());
    };

    if let Some((name, _version)) = fragment.rsplit_once('@') {
        return (!name.is_empty()).then(|| name.to_string());
    }
    // A bare version fragment: the name is the URL's last path segment.
    if fragment.starts_with(|c: char| c.is_ascii_digit()) {
        let last = url.rsplit(['/', '\\']).find(|s| !s.is_empty())?;
        return Some(last.to_string());
    }
    // A bare name fragment (no version) — legal in a PackageIdSpec.
    (!fragment.is_empty()).then(|| fragment.to_string())
}

/// Read cargo's `--message-format=json-render-diagnostics` stream.
///
/// **Pure**, and tested by replaying a stream recorded from a real cargo run
/// rather than by modelling what cargo is believed to emit.
///
/// Three filters earn their place:
///
/// * only `reason: "compiler-artifact"` messages — the stream also carries
///   `compiler-message`, `build-script-executed` and `build-finished`;
/// * `custom-build`, `test`, `bench` and `example` targets are dropped, because a
///   **build script also reports an `executable`** and would otherwise land in
///   the report as if it were a deliverable;
/// * the result is sorted and deduplicated, because a package built for two
///   units in one invocation reports its artifact more than once.
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

// ---------------------------------------------------------------------------
// The duplicate-artifact-name guard — pure core, impure shell
// ---------------------------------------------------------------------------

/// Which output-file class a cargo target kind belongs to.
///
/// `bin` and `staticlib` and the dynamic kinds are separate namespaces on disk
/// (`foo`, `libfoo.a`, `libfoo.dylib`), so two targets only collide when they
/// agree on *both* the class and the name.
fn collision_class(kind: &str) -> Option<&'static str> {
    match kind {
        "bin" => Some("bin"),
        "cdylib" | "dylib" => Some("dylib"),
        "staticlib" => Some("staticlib"),
        _ => None,
    }
}

/// The pure half of [`assert_no_duplicate_artifact_names`]: given a
/// `cargo metadata --no-deps` document, name a collision or return `None`.
///
/// Split out so the rule is testable without a compile *and* without a cargo
/// process, while the caller below still gets its input from real cargo.
///
/// The message **names both packages**, which is the entire requirement: cargo's
/// own report of this condition is a warning that names neither clearly, exits 0,
/// and leaves one of the two files behind.
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

/// Ask cargo to describe the cache workspace: `cargo metadata --no-deps`,
/// **always at the workspace root**.
///
/// The one place in the CLI that spawns `cargo metadata`.  Two callers need it
/// for different reasons — the collision guard below reads `packages`, and
/// [`target_directory`] reads `target_directory` — and a second spawn site is a
/// second chance to ask it from the wrong directory, which is the whole class of
/// defect #335 exists to close.
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

/// The directory cargo would actually write artifacts into for this workspace.
///
/// **Never joined by hand.**  Cargo resolves its target directory from
/// `CARGO_TARGET_DIR` *and* from `[build] target-dir` in every `config.toml` on
/// its discovery chain, including the machine-wide `$CARGO_HOME/config.toml`.
/// A hand-built `<root>/target` is wrong for those users and wrong *silently* —
/// that is #292 verbatim, and it shipped as a broken mandatory upgrade path.
///
/// Used by the one caller that cannot ask cargo what it just produced because it
/// deliberately produces nothing: `migrate run`, which executes a bin an earlier
/// `migrate build` left behind.
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

/// Pre-build collision guard.  **Needs no compile.**
///
/// Runs `cargo metadata --no-deps` over the cache workspace root and refuses to
/// start a build in which two packages would write the same output file.  It runs
/// *before* [`execute`] because the condition it catches is one cargo declines to
/// treat as an error: it warns, exits 0, and silently keeps one file.
pub fn assert_no_duplicate_artifact_names(root: &Path) -> Result<()> {
    let metadata = metadata(root)?;

    match duplicate_artifact_names(&metadata) {
        Some(message) => Err(CliError::Build(message)),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// execute() — the only impure part
// ---------------------------------------------------------------------------

/// Run the planned invocations and return every artifact they produced, each
/// **existence-checked**.
///
/// # Why stderr is teed rather than inherited
///
/// Plan #347 asks for two things that cannot both be literally true: stream
/// stderr with `Stdio::inherit()`, *and* fail on `output filename collision` in
/// stderr.  An inherited stream is never read by this process, so it cannot be
/// inspected.  The resolution is a tee: a reader thread copies cargo's stderr to
/// our stderr line by line as it arrives — so the user still watches the build —
/// and keeps a copy for the collision check.  Nothing is buffered until the end.
/// (Cargo already suppresses its progress bar when stderr is not a terminal, so
/// no rendering is lost that a redirect would not have lost anyway.)
///
/// Reading stdout on this thread while a second thread drains stderr is not
/// stylistic: a child that fills either pipe buffer while the parent reads only
/// the other one deadlocks.
///
/// # Why the paths are checked
///
/// The path is not ours to compute.  Cargo resolves its target directory from
/// `CARGO_TARGET_DIR` **and** from `[build] target-dir` in every `config.toml` on
/// its discovery chain — including the machine-wide `$CARGO_HOME/config.toml` —
/// so a path joined by hand is wrong for those users, and wrong *silently*.  That
/// is #292 verbatim: a constructed path that was never checked let `migrate
/// build` exit 0 while naming a file that was not there.
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
            // Lossy on purpose: cargo's JSON is UTF-8, but a diagnostic embedded
            // in it can carry a user's bytes, and a decode error must not be the
            // thing that fails a successful build.
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

        // Checked even though cargo exited 0 — *especially* because it did.
        // The pre-build guard above is the primary defense; this is the net for
        // a collision it could not see, and it must not be downgraded to a
        // warning: a warning here is exactly what cargo already prints.
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

/// The host target triple, as rustc reports it.
///
/// Asked of rustc rather than guessed from `cfg!` pairs: the report's `triple`
/// field is meant to be pasted into a `--target` flag, and a hand-assembled
/// `x86_64-apple-darwin` is wrong on exactly the machines where it matters.
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

/// Pure half of [`host_triple`].
pub fn parse_host_triple(vv: &str) -> Option<String> {
    vv.lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(|t| t.trim().to_string())
}

// ---------------------------------------------------------------------------
// The machine-readable report (#335 §14/§15)
// ---------------------------------------------------------------------------

/// The cargo profile a report's paths belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Debug,
    Release,
}

/// One artifact, as `--report` writes it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportedArtifact {
    /// The cargo `[package] name` — `naming::package_name`, app name included.
    pub package: String,
    /// `naming::PackageKind::dir()`.  **The stable selector**: a package name is
    /// derived from the app's path and changes when a schema moves, kinds do
    /// not, so this is what a Dockerfile may name.  It round-trips through
    /// `PackageKind::from_dir`.
    pub kind: String,
    pub target_kind: TargetKind,
    /// Absolute, and existence-checked before it got here.
    pub path: PathBuf,
    /// The triple this invocation built for (the host triple when no `--target`
    /// was passed).  Present because the wasm arm is a separate invocation with
    /// a different triple.
    pub triple: String,
}

/// The machine-readable result of one `forgedb build` (#335 §14).
///
/// Serialized by `--report`; `--print-artifact` is a projection of the same
/// value, which is what keeps the two flags from disagreeing.
///
/// **Written only on success**, after every path in it has been
/// existence-checked.  A report left behind by a failed build is a lie a later
/// step reads as truth.
#[derive(Debug, serde::Serialize)]
pub struct BuildReport {
    /// Format version.  `1`.  A consumer that does not recognise it must fail,
    /// not guess.
    pub version: u32,
    /// The ForgeDB-owned workspace root: `~/.forgedb/projects/<id>`.
    pub project: PathBuf,
    /// This app's container: `<project>/apps/<member-hash>`.
    pub app: PathBuf,
    pub profile: Profile,
    /// One entry per emitted artifact FILE.  Empty is legal; absent is not.
    pub artifacts: Vec<ReportedArtifact>,
}

/// The format version `--report` writes.
pub const REPORT_VERSION: u32 = 1;

/// The one artifact `--print-artifact <KIND>` means, for each kind.
///
/// A `ffi` package emits three files that all exist on disk; a `core` package
/// emits an `.rlib` and an `.rmeta`.  Naming the primary kind here is what makes
/// `--print-artifact ffi` mean the **staticlib** — the thing Go links — rather
/// than whichever of the three the parser happened to see first.
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
    /// Serialize as the pretty JSON `--report` writes.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| CliError::Build(format!("could not render the build report: {e}")))
    }

    /// Resolve `--print-artifact <KIND>` against this report.
    ///
    /// **Zero matches, or more than one, is a hard error naming what was
    /// found.**  Silence or a guessed pick here is how a container ships the
    /// wrong binary.
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

    fn render_inventory(&self) -> String {
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
