//! Artifact delivery (#337): the compiled half of every class-B target, copied
//! out of the build cache and into the app's `output`, beside the generated text
//! that describes it.
//!
//! # It is a projection of the build report, never a second inventory
//!
//! Every path here comes from [`BuildReport`], which `driver::execute` built by
//! reading cargo's own JSON stream and existence-checking each file. **Delivery
//! joins no path.** That is #292's defect class — a command that reconstructs
//! `target/<profile>/<name>` is wrong the moment a `[build] target-dir` exists
//! anywhere in the user's cargo config, and it is wrong silently.
//!
//! # Copy, never symlink
//!
//! C8 permits the cache to be garbage-collected at any time, and a symlink into
//! a deleted cache fails at the consumer's *runtime* rather than at build time.
//!
//! Copying is necessary and, for one target, not sufficient: rustc stamps an
//! **absolute** `LC_ID_DYLIB` into a cdylib, and copying a macOS dylib does not
//! rewrite it. So anything a consumer *links* gets the static archive instead —
//! see [`destinations_for`]. Node and CPython `dlopen` their extension by path
//! and record no dependency on it, so a copied cdylib is safe for those two;
//! `tests/delivery_test.rs` scenario 15 asserts that with `otool`/`ldd` rather
//! than trusting the argument.

use std::path::{Path, PathBuf};

use crate::naming::PackageKind;
use crate::{Result, error::CliError, ui};

use super::driver::{BuildReport, DeliveredArtifact, TargetKind};

/// One (package kind, artifact) pair and where its file lands.
pub struct Destination {
    /// The package whose build produced the file.
    pub kind: PackageKind,
    /// **Which** of that package's files. A `ffi` package emits three, and all
    /// three exist on disk, so existence cannot discriminate.
    pub target_kind: TargetKind,
    /// Relative to the app's resolved `output`.
    pub dir: &'static str,
    /// The delivered name — **always a rename**. Cargo writes
    /// `lib<pkg>.dylib`/`.so`; CPython will not import a `.dylib` and Node
    /// requires a `.node`. Do not "simplify" this by copying the basename.
    ///
    /// Owned rather than `&'static str` so the pyo3 row can COMPOSE its name
    /// from `PyO3Generator::EXTENSION_STEM`. A `&'static str` here forces a
    /// literal, and a literal is a second spelling of the stem — which is the
    /// `PyInit_<stem>` mismatch the constant exists to prevent.
    pub file: String,
}

/// Where each package kind's artifacts go.
///
/// **A TOTAL match, with no wildcard arm.** Adding a `PackageKind` is then a
/// compile error here rather than a silent non-delivery, which is the failure
/// this whole issue exists to remove — a target that builds, reports, and
/// reaches nobody.
///
/// The undelivered kinds are listed explicitly for the same reason: an absent
/// arm and an empty arm look identical to a reader and mean opposite things
/// (*never considered* vs *verified to have no destination*).
pub fn destinations_for(kind: &PackageKind) -> Vec<Destination> {
    match kind {
        // The Node-API addon. `dlopen`ed by path, records no dependency on
        // itself, so the cdylib is safe to copy.
        PackageKind::Napi => vec![Destination {
            kind: PackageKind::Napi,
            target_kind: TargetKind::Cdylib,
            dir: "napi",
            file: "forgedb.node".to_string(),
        }],
        // The CPython extension. The name is not cosmetic: CPython resolves
        // `PyInit_<stem>` from the DELIVERED FILENAME, so this must agree with
        // the `#[pymodule]` function name — one constant, read by both.
        PackageKind::Pyo3 => vec![Destination {
            kind: PackageKind::Pyo3,
            target_kind: TargetKind::Cdylib,
            dir: "pyo3",
            file: forgedb_codegen::PyO3Generator::extension_file(),
        }],
        // Two destinations from ONE artifact, and not a mistake: a project that
        // declares both `ffi` and `go` gets a self-contained copy in each. Do
        // not deduplicate — the point of each copy is that its directory needs
        // nothing outside itself.
        //
        // The **staticlib**, never the cdylib. A copied dylib carries the
        // cache's absolute install name, so a C consumer that linked it records
        // a path into a directory C8 permits deleting at any time. An archive's
        // contents are linked in, so there is nothing left to dangle. A shared
        // library for C is deferred on #335's terms (an `@rpath/` install name
        // plus `-Wl,-rpath,@loader_path`, and a smoke test run WITH THE CACHE
        // DELETED — without that guard both legs stay green on the broken shape).
        PackageKind::Ffi => vec![
            Destination {
                kind: PackageKind::Ffi,
                target_kind: TargetKind::Staticlib,
                dir: "ffi",
                file: GO_STATICLIB.to_string(),
            },
            Destination {
                kind: PackageKind::Ffi,
                target_kind: TargetKind::Staticlib,
                dir: "go",
                file: GO_STATICLIB.to_string(),
            },
        ],
        // `core` is an rlib every wrapper links inside the cache; `server` is a
        // binary C8 keeps an explicit copy (`--print-artifact server` reports it
        // in the same invocation that produced it, so the deploy path has no
        // window in which it could have moved); `wasm` is the browser replica,
        // excluded by the epic's own scope note.
        PackageKind::Core | PackageKind::Server | PackageKind::Wasm => Vec::new(),
        // Class C. Written by `migrate build` / `migrate engine`, built and run
        // inside one invocation, and never leaving the cache.
        PackageKind::Transform { .. } | PackageKind::Engine { .. } => Vec::new(),
    }
}

/// The delivered name of the FFI static archive.
///
/// A FIXED name, not the derived package name, because the cgo preamble
/// `crates/codegen/src/go.rs` emits is a `const &str` that must name the library
/// it links: `-L${SRCDIR} -lforgedb`. The archive already sits in a per-app
/// directory, so deriving it would thread a hash into a static template for no
/// benefit.
pub const GO_STATICLIB: &str = "libforgedb.a";

/// Every destination, in a stable order.
///
/// Enumerated from the kinds that can appear in a report rather than from a
/// second list: `destinations_for` is the one table, and this walks it.
fn all_destinations() -> Vec<Destination> {
    [
        PackageKind::Core,
        PackageKind::Server,
        PackageKind::Napi,
        PackageKind::Pyo3,
        PackageKind::Ffi,
        PackageKind::Wasm,
    ]
    .iter()
    .flat_map(destinations_for)
    .collect()
}

/// Deliver everything this build produced that has a destination.
///
/// Two rules, both inherited from the Go carve-out #335 was forced to write
/// rather than invented here:
///
/// * **A destination directory that does not exist is skipped.** There is
///   nothing to deliver *to* — the project did not generate that binding.
/// * **A destination that exists with no matching artifact in the report is a
///   HARD ERROR**, naming what the build did produce. Silence there is how a
///   consumer keeps loading last week's artifact forever.
pub fn run(output: &Path, report: &BuildReport) -> Result<Vec<DeliveredArtifact>> {
    let mut delivered = Vec::new();

    for dest in all_destinations() {
        let dir = output.join(dest.dir);
        if !dir.is_dir() {
            continue;
        }

        let kind_dir = dest.kind.dir();
        let hits: Vec<_> = report
            .artifacts
            .iter()
            .filter(|a| a.kind == kind_dir && a.target_kind == dest.target_kind)
            .collect();

        let source = match hits.as_slice() {
            [one] => *one,
            [] => {
                return Err(CliError::Build(format!(
                    "{} exists but this build produced no `{}` {} to deliver into it.\n\n\
                     It produced:\n{}\n\n\
                     Either the project stopped declaring that target while its output \
                     directory remained, or this is a ForgeDB bug; please report it.",
                    dir.display(),
                    kind_dir,
                    dest.target_kind.as_str(),
                    report.render_inventory()
                )));
            }
            many => {
                return Err(CliError::Build(format!(
                    "{} — {} artifacts match `{}` {}, so which one to deliver is a guess:\n{}",
                    dir.display(),
                    many.len(),
                    kind_dir,
                    dest.target_kind.as_str(),
                    many.iter()
                        .map(|a| format!("  {} ({})", a.path.display(), a.package))
                        .collect::<Vec<_>>()
                        .join("\n")
                )));
            }
        };

        // The report existence-checked this path when it was built. Checking it
        // again here is not redundancy: a build can be followed by a cache GC in
        // the same invocation's lifetime, and `fs::copy`'s own error names the
        // syscall rather than the situation.
        if !source.path.is_file() {
            return Err(CliError::Build(format!(
                "the build reported `{}` at {}, and that file is not there now.\n\
                 ForgeDB does not reconstruct artifact paths — it reads every one from \
                 the report — so this is a file that moved or was deleted after cargo \
                 wrote it, not a path ForgeDB guessed.",
                kind_dir,
                source.path.display()
            )));
        }

        let to = dir.join(&dest.file);
        std::fs::copy(&source.path, &to).map_err(|e| {
            CliError::Build(format!(
                "failed to deliver {} to {}: {e}",
                source.path.display(),
                to.display()
            ))
        })?;

        // C7: every delivered path is printed.
        ui::info(&format!(
            "{} ({}): {}",
            dest.dir,
            dest.target_kind.as_str(),
            to.display()
        ));

        delivered.push(DeliveredArtifact {
            kind: kind_dir,
            target_kind: dest.target_kind,
            from: source.path.clone(),
            to,
        });
    }

    Ok(delivered)
}

/// Deliver into `output` only if it is `Some` — the shape `build` calls.
pub fn run_if_output(output: Option<&str>, report: &BuildReport) -> Result<Vec<DeliveredArtifact>> {
    let Some(output) = output else {
        return Ok(Vec::new());
    };
    run(&PathBuf::from(output), report)
}
