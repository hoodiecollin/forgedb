//! **The emitted `core` package must compile when the invocation narrows** (#445).
//!
//! # The defect, and why nothing already in the tree could see it
//!
//! `core/src/lib.rs` and `core/Cargo.toml` are a matched pair: the source writes
//! `use utoipa::ToSchema;` and the manifest pins `utoipa`. They shipped reading
//! **two different conditions** —
//!
//! * the derive and the import on `GenConfig::web`, the app's **declared**
//!   target set (`crates/codegen/src/rust.rs`);
//! * the pin on `cache.api.is_some()`, whether *this invocation* happened to
//!   emit an `api.rs` (`src/commands/generate/mod.rs`).
//!
//! Those agree for `generate all` and disagree for every invocation that
//! narrows. `forgedb generate rust` under `targets = ["all"]` writes a `core`
//! that does not build:
//!
//! ```text
//! error[E0432]: unresolved import `utoipa`
//!  --> apps/<hash>/core/src/lib.rs:9:5
//! ```
//!
//! **No string assertion in the tree could catch it, and that is the point.**
//! Both halves are individually well-formed: a manifest without a `utoipa` line
//! is a perfectly good manifest, and `use utoipa::ToSchema;` is a perfectly good
//! import. Only their *disagreement* is a defect, so a snapshot of either one is
//! green. `crates/codegen/tests/codegen_snapshots.rs` never sees the manifest at
//! all, `tests/cache_manifest_deps_test.rs` cross-checks the **server** package
//! (not `core`) and drives the generators directly rather than the CLI — and the
//! wrong condition lives in the CLI, at the call site. What is left is to
//! compile what ForgeDB actually wrote.
//!
//! # Two halves, per the convention in `tests/build_cache_compile_test.rs`
//!
//! The cheap half — is the pair *consistent* in the emitted files — runs in
//! tier 1. The half that runs cargo over the cache workspace is `#[ignore]`d
//! into tier 2 (`make test-ignored`), like the twelve other tests that compile a
//! generated crate.
//!
//! Cargo is never mocked here. The whole claim is "this compiles", and a mock of
//! a compiler asserts nothing about a compiler.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One model is enough: the derive is per-model but the *import* is per-crate,
/// and the import is what fails to resolve. A wider schema would compile more
/// slowly and prove the same thing.
const SCHEMA: &str = r#"
User {
  id: +uuid
  email: &string
  name: string
}
"#;

/// **`targets = ["all"]` is the load-bearing half of this fixture.** It is what
/// makes `GenConfig::web` true, so that a `generate rust` — which emits no
/// `api.rs` — puts the two conditions on opposite sides.
const CONFIG: &str = r#"[generate]
targets = ["all"]
"#;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// A temp project that has run one narrowing `generate`, plus the cache it wrote.
struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Fixture {
    /// Write the project and run `forgedb generate <target>` once.
    ///
    /// `FORGEDB_HOME` is redirected into the tempdir: without it `generate`
    /// claims a project id in the developer's real `~/.forgedb` ledger and
    /// writes its cache outside the fixture.
    fn generate(target: &str) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("proj");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&proj).unwrap();
        write(&proj.join("forgedb.toml"), CONFIG);
        write(&proj.join("schema.forge"), SCHEMA);

        let out = Command::new(env!("CARGO_BIN_EXE_forgedb"))
            .args(["generate", target, "--schema", "schema.forge"])
            .current_dir(&proj)
            .env("FORGEDB_HOME", &home)
            .output()
            .expect("run forgedb generate");
        assert!(
            out.status.success(),
            "`forgedb generate {target}` failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );

        Fixture { _tmp: tmp, home }
    }

    fn project_root(&self) -> PathBuf {
        let projects = self.home.join("projects");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&projects)
            .unwrap_or_else(|e| panic!("no cache at {}: {e}", projects.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), 1, "expected one project in the cache: {dirs:?}");
        dirs.pop().unwrap()
    }

    fn core_dir(&self) -> PathBuf {
        let apps = self.project_root().join("apps");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&apps)
            .unwrap_or_else(|e| panic!("no apps at {}: {e}", apps.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), 1, "expected one app container: {dirs:?}");
        dirs.pop().unwrap().join("core")
    }

    /// Point every substrate crate at this checkout, so the compile below builds
    /// the source in front of us rather than whatever is published.
    ///
    /// Proving the *published* substrate resolves is
    /// `.github/workflows/substrate-reclose.yml`'s job, and it stays there: that
    /// is the only check that runs outside this repo.
    fn patch_substrate(&self) {
        let manifest = self.project_root().join("Cargo.toml");
        let mut body = read(&manifest);
        body.push_str("\n[patch.crates-io]\n");
        for dir in [
            "storage",
            "storage-native",
            "storage-web",
            "types",
            "changefeed",
            "wal",
            "compaction",
            "txn",
            "coordinator",
        ] {
            let path = repo_root().join("crates").join(dir);
            assert!(path.is_dir(), "no such substrate crate: {}", path.display());
            body.push_str(&format!(
                "forgedb-{dir} = {{ path = {:?} }}\n",
                path.to_string_lossy()
            ));
        }
        std::fs::write(&manifest, body).unwrap();
    }

    /// `cargo check` over the cache workspace.
    ///
    /// `--target-dir` is explicit and `CARGO_TARGET_DIR` is removed: an ambient
    /// env var — or a machine-wide `[build] target-dir` in
    /// `$CARGO_HOME/config.toml`, which needs no env var at all (#292) — would
    /// redirect this into the directory the outer `cargo test` holds a lock on,
    /// and the test would hang rather than fail.
    fn cargo_check(&self) -> std::process::Output {
        Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["check", "--workspace", "--target-dir"])
            .arg(self.home.join("cargo-target"))
            .current_dir(self.project_root())
            .env_remove("CARGO_TARGET_DIR")
            .output()
            .expect("cargo runs")
    }
}

/// Whether the emitted manifest pins `utoipa` as a dependency.
///
/// Anchored on the dependency line rather than on the word, which also appears
/// in the manifest's own header comment on no version of this file — but would
/// be a silent false positive the moment one is added.
fn pins_utoipa(manifest: &str) -> bool {
    manifest
        .lines()
        .any(|l| l.trim_start().starts_with("utoipa ") || l.trim_start().starts_with("utoipa="))
}

/// Whether the emitted source names the crate the manifest has to pin.
fn imports_utoipa(source: &str) -> bool {
    source.contains("use utoipa::")
}

// ---------------------------------------------------------------------------
// Tier 1 — the cheap half: the two files agree
// ---------------------------------------------------------------------------

/// **`generate rust` under `targets = ["all"]`** — the reported reproduction.
///
/// The assertion is an `assert_eq!` between the two halves rather than two
/// independent `assert!`s on purpose: what is wrong is neither value on its own,
/// it is that they can differ. Written as two assertions, a future change that
/// flipped *both* would read as two failures rather than as one.
#[test]
fn a_narrowing_generate_leaves_the_pin_and_the_import_agreeing() {
    let fx = Fixture::generate("rust");
    let core = fx.core_dir();
    let manifest = read(&core.join("Cargo.toml"));
    let source = read(&core.join("src/lib.rs"));

    assert_eq!(
        pins_utoipa(&manifest),
        imports_utoipa(&source),
        "core/Cargo.toml and core/src/lib.rs disagree about utoipa (#445).\n\
         pins = {}, imports = {}.\n\
         The emitted crate does not compile: `error[E0432]: unresolved import \
         `utoipa``.\n\
         Both sides must read GenConfig::needs_utoipa — see \
         crates/codegen/src/config.rs.\n\n\
         {manifest}",
        pins_utoipa(&manifest),
        imports_utoipa(&source),
    );
}

/// The fixture must not be vacuous. `targets = ["all"]` is what makes
/// `GenConfig::web` true, and if it ever stopped doing so this whole file would
/// pass by testing the uninteresting `web = false` case in both halves.
///
/// So: assert the derive really is present. That is the state under which the
/// missing pin was fatal.
#[test]
fn the_fixture_really_does_declare_a_web_surface() {
    let fx = Fixture::generate("rust");
    let source = read(&fx.core_dir().join("src/lib.rs"));
    assert!(
        imports_utoipa(&source),
        "`targets = [\"all\"]` no longer makes the Rust core derive ToSchema — \
         this file's scenario has evaporated and the compile test below proves \
         nothing. Fix the fixture rather than deleting the assertion."
    );
}

// ---------------------------------------------------------------------------
// Tier 2 — the half that compiles (`make test-ignored`)
// ---------------------------------------------------------------------------

/// **The guard.** Compile the `core` package ForgeDB wrote for a project that
/// declares a web target while generating only the Rust core.
///
/// This is the assertion the two conditions can never satisfy while they are two
/// conditions, and no cheaper form of it exists — see the module docs.
#[test]
#[ignore = "compiles a generated crate with real cargo; tier 2 (`make test-ignored`)"]
fn the_core_a_narrowing_generate_writes_compiles() {
    let fx = Fixture::generate("rust");
    fx.patch_substrate();

    let out = fx.cargo_check();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the `core` package written by `forgedb generate rust` under \
         `targets = [\"all\"]` does not compile (#445).\n\n\
         core/Cargo.toml:\n{}\n\ncargo says:\n{stderr}",
        read(&fx.core_dir().join("Cargo.toml")),
    );
}
