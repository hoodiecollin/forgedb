//! Guards for the migration commands' handling of **cargo's target directory**
//! (#292).
//!
//! `migrate build`, `up`, `run` and `engine` all have to name the transformer
//! binary cargo produced. Three of them used to *guess* it as
//! `<output>/target/release/forgedb-transform`, which is wrong the moment the
//! target directory is redirected — by `CARGO_TARGET_DIR`, or by a `[build]
//! target-dir` in any `config.toml` on cargo's discovery chain (including
//! `$CARGO_HOME/config.toml`, i.e. machine-wide). `migrate build` then exited **0**
//! while reporting a path that did not exist, and `migrate engine` — the step
//! `docs/UPGRADING.md` makes mandatory for the 0.4.0 engine hop — died on a bare
//! `os error 2` naming no path at all.
//!
//! **The assertion has to be on the artifact, not the exit status**: the defect's
//! signature is a *successful* exit reporting a path that is not there, so
//! `status.success()` cannot separate fixed from broken.
//!
//! **Why the stub crate.** The property under test is "does the CLI name the path
//! cargo actually wrote", so cargo itself must be real — mocking it here would test
//! our assumption about cargo against itself, and that assumption *is* the bug. But
//! the real transformer source is irrelevant to that property and drags in the
//! whole published substrate closure, so these tests pre-seed a dependency-free
//! crate with one bin named `forgedb-transform`. `emit_transform`/`emit_engine`
//! write `Cargo.toml` only-if-absent, so the seed survives; `autobins = false`
//! keeps the generated `src/main.rs` they *do* rewrite from being picked up as a
//! second target. Real cargo, real artifact resolution, an instant build.
//!
//! The end-to-end counterpart — the real transformer, compiled against the
//! *published* substrate, under a genuinely shared target dir — is step 6/6 of
//! `.github/workflows/substrate-reclose.yml`, which is where this bug was found.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const V1: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n}\n";
const V2: &str = "Widget {\n  id: +uuid\n  sku: &string\n  qty: u32\n  note: string?\n}\n";

/// A `forgedb` invocation scoped to `dir`, with cargo's target dir redirected to
/// `target` — the configuration the whole bug is about. Explicit `current_dir`
/// keeps this hermetic and parallel-safe (the migrate commands resolve
/// `migrations/` from the process working directory).
fn forgedb(dir: &Path, target: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    // #333: keep any project-id claim inside the fixture's own tempdir.
    cmd.env("FORGEDB_HOME", dir.join(".forgedb-home"));
    cmd.current_dir(dir).env("CARGO_TARGET_DIR", target);
    cmd
}

/// Seed a crate cargo can build with no network and no dependencies, exposing a
/// bin under the name the CLI looks for.
fn seed_stub_crate(dir: &Path, package: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\n\
             name = \"{package}\"\n\
             version = \"0.0.0\"\n\
             edition = \"2021\"\n\
             autobins = false\n\
             \n\
             [[bin]]\n\
             name = \"forgedb-transform\"\n\
             path = \"stub.rs\"\n\
             \n\
             [workspace]\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("stub.rs"), "fn main() {}\n").unwrap();
}

/// Record a `v1 -> v2` lineage, which `migrate build` needs before it can emit
/// anything at all.
fn record_lineage(dir: &Path, target: &Path) {
    fs::write(dir.join("schema.forge"), V2).unwrap();
    fs::write(dir.join("v1.forge"), V1).unwrap();
    fs::write(dir.join("v2.forge"), V2).unwrap();
    for (name, schema) in [("baseline", "v1.forge"), ("add_note", "v2.forge")] {
        let out = forgedb(dir, target)
            .args(["migrate", "create", name, "--auto", "--schema", schema])
            .output()
            .expect("run migrate create");
        assert!(
            out.status.success(),
            "recording the {name} hop failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// #292: `migrate build` must report the path cargo wrote, and `migrate run` must
/// then find it, with the target directory redirected.
#[test]
fn test_migrate_build_reports_the_path_cargo_actually_wrote() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let target = dir.join("shared-target");
    fs::create_dir_all(&target).unwrap();

    record_lineage(dir, &target);
    seed_stub_crate(&dir.join("migrations/transform"), "forgedb-transform");

    let out = forgedb(dir, &target)
        .args(["migrate", "build", "--from", "1", "--to", "2"])
        .output()
        .expect("run migrate build");
    let log = combined(&out);
    assert!(out.status.success(), "migrate build failed:\n{log}");

    // The premise: cargo honours the redirect. If this fails the test is wrong,
    // not the CLI.
    let real: PathBuf = target.join("release/forgedb-transform");
    assert!(
        real.is_file(),
        "premise broken — cargo did not write the bin into the redirected target dir:\n{log}"
    );

    // The defect: exit 0 with a path that is not there. Assert on the artifact.
    assert!(
        log.contains(&real.display().to_string()),
        "migrate build must report the path cargo wrote ({}), not a guess:\n{log}",
        real.display()
    );
    assert!(
        !dir.join("migrations/transform/target").exists(),
        "nothing was ever written to the guessed location, so reporting it is a lie:\n{log}"
    );

    // `migrate run` does not build — it has to *locate* what an earlier build
    // produced, which is a different derivation from the same fact.
    let out = forgedb(dir, &target)
        .args([
            "migrate",
            "run",
            "--src",
            "./data",
            "--dest",
            "./data-migrated",
        ])
        .output()
        .expect("run migrate run");
    let log = combined(&out);
    assert!(
        !log.contains("transformer bin not found"),
        "migrate run sends the user back to the build they just ran:\n{log}"
    );
    assert!(out.status.success(), "migrate run failed:\n{log}");
}

/// #292: `migrate engine` — the mandatory 0.4.0 hop — had no existence check at
/// all, so a redirected target dir surfaced as `failed to run transformer: No such
/// file or directory (os error 2)`, naming no path.
#[test]
fn test_migrate_engine_finds_the_hop_bin_under_a_redirected_target_dir() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let target = dir.join("shared-target");
    fs::create_dir_all(&target).unwrap();

    fs::write(dir.join("schema.forge"), V1).unwrap();

    // A pre-0.4.0 data dir: one model manifest carrying `format_version` and no
    // `engine_version`, which `detect_src_versions` baselines to generation 1.
    fs::create_dir_all(dir.join("data-gen1/Widget")).unwrap();
    fs::write(
        dir.join("data-gen1/Widget/manifest.json"),
        r#"{"format_version": 1, "row_count": 0, "columns": []}"#,
    )
    .unwrap();

    seed_stub_crate(&dir.join("migrations/engine"), "forgedb-engine-migrate");

    let out = forgedb(dir, &target)
        .args([
            "migrate",
            "engine",
            "--src",
            "./data-gen1",
            "--dest",
            "./data-gen2",
        ])
        .output()
        .expect("run migrate engine");
    let log = combined(&out);

    assert!(
        !log.contains("failed to run transformer"),
        "migrate engine execs a path it guessed instead of the one cargo wrote:\n{log}"
    );
    assert!(out.status.success(), "migrate engine failed:\n{log}");
}
