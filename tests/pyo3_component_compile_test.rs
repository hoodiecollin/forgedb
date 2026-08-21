//! **Scenario 23 (#347).** *A schema with a component ref emits compilable pyo3.*
//!
//! *Given* a schema declaring a component ref · *When* the python runtime binding
//! is generated · *Then* it **builds** — not merely checks.
//!
//! # Why this has to be a build, and why it has to be this schema
//!
//! Two shipped defects meet here, and each is invisible to the guard that would
//! catch the other.
//!
//! 1. **`needs_py_lifetime` omitted `FieldType::Component`** (`pyo3.rs`). A
//!    component field falls through `pyo3_getter`'s wildcard to the pythonize
//!    path, whose return type is `Bound<'py, PyAny>` — but the signature was
//!    emitted without `<'py>` and without `py`, so the getter named an undeclared
//!    lifetime and an undefined value. Nine hard errors. `syn` parses the broken
//!    form happily, so codegen **succeeds** and a snapshot diff shows nothing
//!    wrong; only a compiler sees it. CI never did, because the reclose schema
//!    has no component ref — hence the schema below.
//!
//! 2. **There was no `build.rs` at all.** `extension-module` deliberately does
//!    not link libpython, and the macOS linker then rejects the cdylib's
//!    undefined `_PyModule_Create2` unless it is passed `-undefined
//!    dynamic_lookup`. Linux `ld -shared` permits undefined symbols, so this is a
//!    **runner-OS** discriminator rather than a check-vs-build one — and the
//!    reclose ran `cargo check`, which never links at all, so it was invisible
//!    twice over.
//!
//! Defect 1 is a type-check failure and defect 2 is a link failure. `cargo check`
//! catches neither on Linux and only the first on macOS. A `cargo build` on the
//! host, over a schema with a component ref, catches both wherever it runs.
//!
//! # Shape
//!
//! The test assembles the two cache packages this app would get — `core` (the
//! generated database, as a library) and `pyo3` (the wrapper that links it) — in
//! a throwaway workspace, then builds the wrapper. That is deliberately the real
//! layered shape rather than a single flattened crate: the wrapper pins **zero**
//! substrate and reaches `forgedb_types::Uuid` / `forgedb_storage::ColumnExport`
//! only through `core`'s re-exports, so a flattened crate would not exercise the
//! thing that makes those types unify.
//!
//! The substrate is `[patch.crates-io]`-ed to this checkout. Proving that a
//! *published* substrate resolves is the reclose workflow's job (and it must stay
//! there — it is the only check that runs outside this repo); what this test owes
//! is that the emitted source compiles, against the substrate as it is now.

use std::path::{Path, PathBuf};
use std::process::Command;

const COMPONENT_SCHEMA: &str = r#"
User {
  id: +uuid
  email: string
  views: i32
  card: tsx://components/user/card
  profile: jsx://components/profile @relations(*)
  verify: api://routes/user/verify
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

const CORE_PKG: &str = "s23-component-core";
const PYO3_PKG: &str = "s23-component-pyo3";

/// The `pub use` block `commands::generate` appends to `core/src/lib.rs`.
///
/// **Imported, not duplicated.** A local copy went stale within one session: the
/// production list gained `pub use forgedb_changefeed;` (the wasm replica names
/// `forgedb_core::forgedb_changefeed::durable::PersistedEvent`) and a duplicate
/// here would have kept compiling while testing a `core` nothing emits. Two
/// copies of a re-export list is exactly the drift shape #335 is about.
use forgedb::commands::generate::CORE_SUBSTRATE_REEXPORTS;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Point every substrate pin at this checkout, so the test compiles the source in
/// front of it rather than whatever happens to be published.
fn patch_section() -> String {
    let root = repo_root();
    let mut out = String::from("\n[patch.crates-io]\n");
    for (krate, dir) in [
        ("forgedb-storage", "storage"),
        ("forgedb-types", "types"),
        ("forgedb-changefeed", "changefeed"),
        ("forgedb-wal", "wal"),
        ("forgedb-compaction", "compaction"),
        ("forgedb-txn", "txn"),
        ("forgedb-coordinator", "coordinator"),
    ] {
        let path = root.join("crates").join(dir);
        assert!(path.is_dir(), "no such substrate crate: {}", path.display());
        out.push_str(&format!(
            "{krate} = {{ path = {:?} }}\n",
            path.to_string_lossy()
        ));
    }
    out
}

#[test]
#[ignore = "compiles a generated cache workspace; run with --ignored"]
fn scenario_23_a_component_ref_schema_emits_pyo3_that_builds() {
    let mut parser = forgedb_parser::Parser::new(COMPONENT_SCHEMA).unwrap();
    let schema = parser.parse().unwrap();

    // The component field must actually have survived parsing, or this test
    // passes vacuously against the schema class it exists to cover.
    let user = &schema.models[0];
    assert!(
        user.fields.iter().any(|f| matches!(
            f.field_type,
            forgedb_parser::FieldType::Component(_)
        )),
        "the fixture schema has no component ref"
    );

    let database = forgedb_codegen::RustGenerator::generate(&schema).unwrap().code;
    let wrapper = forgedb_codegen::PyO3Generator::generate(&schema).unwrap().code;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    write(
        &root.join("Cargo.toml"),
        &format!(
            "[workspace]\nresolver = \"3\"\nmembers = [\"core\", \"pyo3\"]\n{}",
            patch_section()
        ),
    );

    // `core` — the generated database as a library, plus the substrate
    // re-exports its dependents reach the substrate through. `web` is on by
    // default in `GenConfig`, so the emission carries `ToSchema` derives and the
    // manifest has to pin utoipa for them.
    write(
        &root.join("core/Cargo.toml"),
        &forgedb_codegen::CorePackage::cargo_toml(CORE_PKG, true),
    );
    write(
        &root.join("core/src/lib.rs"),
        &format!("{database}{CORE_SUBSTRATE_REEXPORTS}"),
    );

    // `pyo3` — the wrapper. Zero substrate pins; `core` is its only ForgeDB
    // dependency, and the build script is what makes the macOS link succeed.
    write(
        &root.join("pyo3/Cargo.toml"),
        &forgedb_codegen::PyO3Generator::cargo_toml(PYO3_PKG, CORE_PKG),
    );
    write(&root.join("pyo3/src/lib.rs"), &wrapper);
    write(
        &root.join("pyo3/build.rs"),
        forgedb_codegen::PyO3Generator::build_rs_scaffold(),
    );

    // `--target-dir` explicitly, and `CARGO_TARGET_DIR` removed: an ambient env
    // var *or* a `[build] target-dir` in `$CARGO_HOME/config.toml` would
    // otherwise redirect this build into the target directory the outer
    // `cargo test` is holding a lock on, and the test would hang rather than
    // fail. (#292's hazard class, pointed the other way.)
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "-p", PYO3_PKG, "--target-dir"])
        .arg(root.join("target"))
        .current_dir(root)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("cargo runs");

    assert!(
        out.status.success(),
        "the generated pyo3 wrapper does not build:\n--- stderr ---\n{}\n--- lib.rs ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        wrapper,
    );

    // A build that linked nothing proves nothing about the link args, and the
    // missing `build.rs` was a LINK failure. Assert the cdylib is on disk.
    let libdir = root.join("target/debug");
    let produced: Vec<String> = std::fs::read_dir(&libdir)
        .expect("target/debug exists")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let stem = PYO3_PKG.replace('-', "_");
    assert!(
        produced.iter().any(|n| {
            n.contains(&stem)
                && (n.ends_with(".dylib") || n.ends_with(".so") || n.ends_with(".dll"))
        }),
        "no cdylib was linked for {PYO3_PKG}; the build did not reach the linker: {produced:?}"
    );
}
