//! Shared harness for the tests that prove things about **generated code by
//! running it** — generate a schema, compile the emitted crate against the
//! in-tree substrate, and execute a driver binary that exercises it.
//!
//! A codegen snapshot compares emitted *strings*; it cannot tell you whether the
//! output compiles, let alone whether it behaves. Both of this module's consumers
//! exist because that gap is where real bugs live:
//!
//! - `api_wire_test` — the REST response bytes of every read path.
//! - `list_scan_test` — the ids and `total` the list path selects (#228).
//!
//! The dependency list below is the one thing worth sharing: it mirrors the
//! `forgedb init` server scaffold, and a copy of it in each test file would drift
//! the moment the scaffold gains a dep.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Repo root — `CARGO_MANIFEST_DIR` is the crate this test compiles under.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Path dep line for a workspace substrate crate.
fn dep(name: &str, crate_dir: &str) -> String {
    let path = repo_root().join("crates").join(crate_dir);
    format!("{name} = {{ path = {:?} }}\n", path.to_string_lossy())
}

pub fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// The generated project's `Cargo.toml`: every substrate crate by path (so the
/// test proves the *working tree*, not the registry — the outside-repo reclose in
/// `.github/workflows/substrate-reclose.yml` is what proves the registry), plus
/// the third-party deps the `forgedb init` scaffold pins.
fn cargo_toml(name: &str) -> String {
    let mut s = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n"
    );
    for (n, d) in [
        ("forgedb-storage", "storage"),
        ("forgedb-types", "types"),
        ("forgedb-changefeed", "changefeed"),
        ("forgedb-wal", "wal"),
        ("forgedb-auth", "auth"),
        ("forgedb-query-params", "query-params"),
        ("forgedb-compaction", "compaction"),
        ("forgedb-txn", "txn"),
        ("forgedb-coordinator", "coordinator"),
    ] {
        s.push_str(&dep(n, d));
    }
    s.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
    s.push_str("serde_json = \"1\"\n");
    s.push_str("regex = \"1\"\n");
    s.push_str("rust_decimal = { version = \"1\", features = [\"serde-with-str\"] }\n");
    s.push_str("utoipa = { version = \"5\", features = [\"uuid\"] }\n");
    s.push_str("utoipa-axum = \"0.2\"\n");
    s.push_str("axum = { version = \"0.8\", features = [\"ws\"] }\n");
    s.push_str("tokio = { version = \"1\", features = [\"full\"] }\n");
    s.push_str("tower = { version = \"0.5\", features = [\"util\"] }\n");
    s.push_str("tower-http = { version = \"0.6\", features = [\"trace\", \"cors\"] }\n");
    s.push_str("\n[workspace]\n");
    s
}

/// Generate `schema`, mount `driver` as the crate root exactly as the `forgedb
/// init` scaffold does, compile, and run it with the data dir as `argv[1]`.
///
/// Panics with the tool's own output on any failure before the driver runs, so a
/// generate/compile break reads as itself rather than as a mysterious assertion.
/// The driver's `Output` comes back for the caller to assert on; the project dir
/// is left in place on failure and removed by `cleanup` on success.
pub fn generate_compile_run(tag: &str, schema: &str, driver: &str) -> (Output, PathBuf) {
    let proj = std::env::temp_dir().join(format!("forgedb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(&proj.join("schema.forge"), schema);
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let generated = Command::new(forgedb)
        .args(["generate", "all", "--output", "src", "--schema", "schema.forge"])
        .current_dir(&proj)
        .output()
        .expect("run forgedb generate");
    assert!(
        generated.status.success(),
        "forgedb generate all failed:\n{}\n{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );

    write(&proj.join("Cargo.toml"), &cargo_toml(tag));
    // `generate all` writes database.rs / api.rs into src/; the driver is the
    // crate root that mounts them, exactly as the `forgedb init` scaffold does.
    write(&proj.join("src/main.rs"), driver);

    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run cargo build");
    assert!(
        build.status.success(),
        "driver failed to compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    // `argv[1]` is the data dir. `argv[2]` is the `forgedb` binary, for a driver
    // that needs to drive the CLI itself — the Tier-3 probe spawns
    // `forgedb coordinate` and then re-execs *itself* as two writer processes,
    // which is the only way to prove a multi-process property from one test.
    // Additive: drivers that read only `argv[1]` are unaffected.
    let out = Command::new(target.join(format!("debug/{tag}")))
        .arg(proj.join("data"))
        .arg(forgedb)
        .output()
        .expect("run driver");
    (out, proj)
}

/// Re-run the build over an already-built project and hand back its diagnostics.
///
/// `generate_compile_run` only asserts the build *succeeded*, so it drops stderr on
/// the success path — and a warning is, by definition, on the success path. Some
/// generated-code defects are visible ONLY as a warning in the user's crate: they
/// compile, they behave correctly, and no snapshot diff shows them, because the
/// warning is a property of the emitted code rather than of any value it produces.
///
/// The rebuild is fully cached, so this replays the recorded diagnostics rather
/// than recompiling. Call it BEFORE `assert_driver_ok`, which removes the project.
///
/// Assert on a *targeted* substring, never on "no warnings at all": generated code
/// carries pre-existing benign warnings (unused `record`/`rows` bindings in arms a
/// given schema does not exercise), so a blanket deny would fail for reasons that
/// have nothing to do with the property under test.
pub fn build_warnings(proj: &Path) -> String {
    let out = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(proj)
        .env("CARGO_TARGET_DIR", proj.join("target"))
        .output()
        .expect("re-run cargo build to replay diagnostics");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Print the driver's output, assert it succeeded, and remove the project.
pub fn assert_driver_ok(out: &Output, proj: &Path, what: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}");
    assert!(out.status.success(), "{what}:\n{stdout}\n{stderr}");
    let _ = std::fs::remove_dir_all(proj);
}
