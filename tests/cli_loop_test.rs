//! The loop a user actually runs: `generate` again after editing the schema,
//! and a machine-readable stdout that carries only the machine's payload.
//!
//! Both properties were broken at once and neither had a test, which is not a
//! coincidence — every caller inside this repo had already routed around them.
//! `forgedb generate` refused to overwrite its own output without `--force`, so
//! the Makefile passed `--force`, the reclose harness passed `--force` and so
//! did roughly twenty tests; `build` and `dev` set it internally. Nobody ran the
//! bare command, so nobody saw that the README, `migrate create`'s printed next
//! steps and `generate --check`'s own remedy line all prescribed a command that
//! exits 1.
//!
//! **Tier 1** is everything that compiles nothing. **Tier 2** (`#[ignore]`,
//! `make test-ignored`) drives a real `forgedb build`, because the stray
//! newlines only reach stdout through the full validate → generate → build
//! chain and `--plan` short-circuits before any of it.

mod common;

use common::write;
use std::path::Path;
use std::process::{Command, Output};

const SCHEMA: &str = r#"
User {
  id: +uuid
  email: ^&string @email
  created_at: +timestamp
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

/// A project whose `FORGEDB_HOME` is inside its own tempdir: without the
/// override `generate` claims a project id in the developer's real ledger.
fn project(dir: &Path, targets: &str) {
    std::fs::create_dir_all(dir).unwrap();
    write(&dir.join("schema.forge"), SCHEMA);
    write(
        &dir.join("forgedb.toml"),
        &format!(
            "[project]\nid = \"cliloop\"\nisolated = true\n\n\
             [generate]\noutput = \"generated\"\ntargets = [{targets}]\n"
        ),
    );
}

fn forgedb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forgedb"))
        .args(args)
        .current_dir(dir)
        .env("FORGEDB_HOME", dir.join(".home"))
        .output()
        .expect("run forgedb")
}

fn ok(out: &Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} exited {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------------------
// `generate` is idempotent
// ---------------------------------------------------------------------------

/// Running `generate` a second time succeeds, with no `--force`.
///
/// The THIRD run is not padding. A guard that stops at two passes against an
/// implementation that alternates — write, refuse, write — and the shape this
/// replaced (`if path.exists()`) is one state bit away from exactly that.
#[test]
fn generate_runs_again_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    project(root, "\"rust\"");

    for run in 1..=3 {
        let out = forgedb(root, &["generate", "--schema", "schema.forge"]);
        ok(&out, &format!("generate run {run}"));
    }
}

/// …and the second run's bytes reflect the CURRENT schema.
///
/// Succeeding is not enough on its own: a `generate` that silently skipped when
/// the output already existed would also exit 0 three times, and would leave a
/// user editing a schema that never reaches the generated code. So the schema
/// grows a model between runs and the output has to grow with it.
#[test]
fn generate_rewrites_from_the_edited_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    project(root, "\"rust\"");
    let db_rs = root.join("generated").join("database.rs");

    ok(
        &forgedb(root, &["generate", "--schema", "schema.forge"]),
        "first generate",
    );
    let first = std::fs::read_to_string(&db_rs).expect("database.rs after the first run");
    assert!(
        !first.contains("pub struct Comment"),
        "the fixture must not already carry the model the edit adds"
    );

    write(
        &root.join("schema.forge"),
        &format!("{SCHEMA}\nComment {{\n  id: +uuid\n  body: string\n}}\n"),
    );
    ok(
        &forgedb(root, &["generate", "--schema", "schema.forge"]),
        "generate after the edit",
    );

    let second = std::fs::read_to_string(&db_rs).expect("database.rs after the second run");
    assert!(
        second.contains("pub struct Comment"),
        "the second run did not pick up the edited schema — it exited 0 having \
         written nothing"
    );
}

/// `--force` stays accepted. It is in the Makefile, in the reclose harness and
/// in an unknown number of user Dockerfiles; a removal that turned those red
/// would be a worse outcome than the flag it deletes.
#[test]
fn force_is_still_accepted_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    project(root, "\"rust\"");

    ok(
        &forgedb(root, &["generate", "--schema", "schema.forge"]),
        "plain generate",
    );
    let plain = std::fs::read_to_string(root.join("generated").join("database.rs")).unwrap();
    ok(
        &forgedb(root, &["generate", "--schema", "schema.forge", "--force"]),
        "generate --force",
    );
    let forced = std::fs::read_to_string(root.join("generated").join("database.rs")).unwrap();
    assert_eq!(plain, forced, "`--force` must not change a generated byte");
}

// ---------------------------------------------------------------------------
// A quiet stdout is EMPTY
// ---------------------------------------------------------------------------

/// `--quiet` leaves stdout byte-empty.
///
/// This is the same gate `build --print-artifact` / `build --report -` raise —
/// `main.rs` reaches machine-readable mode by calling `ui::set_verbosity(false,
/// true)`, which is what `--quiet` sets. So an ungated `println!()` anywhere in
/// the validate → generate chain lands in the payload, and it did: `validate`
/// decorated its model/field/relation summary with two bare `println!()` that
/// no verbosity check covered, so `forgedb build --print-artifact server`
/// emitted `\n\n<path>\n`. `$(…)` strips those and `read -r` does not, which is
/// why the Dockerfile ForgeDB scaffolds worked while `head -1` returned nothing.
#[test]
fn quiet_stdout_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    project(root, "\"rust\"");

    for args in [
        vec!["--quiet", "validate", "--schema", "schema.forge"],
        vec!["--quiet", "generate", "--schema", "schema.forge"],
    ] {
        let out = forgedb(root, &args);
        ok(&out, &format!("{args:?}"));
        assert!(
            out.stdout.is_empty(),
            "`{args:?}` wrote {} byte(s) to a quiet stdout: {:?}",
            out.stdout.len(),
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

// ---------------------------------------------------------------------------
// Tier 2 — the real thing
// ---------------------------------------------------------------------------

/// `build --print-artifact <kind>` puts ONE line on stdout and nothing else.
///
/// Tier 2 because it compiles a release cargo workspace. `--plan` cannot stand
/// in: it conflicts with `--print-artifact` and the run exits before the build
/// chain that does the printing.
///
/// The assertion is on the LINE COUNT, not on `contains`. A `contains` check
/// passes with an arbitrary amount of chatter around the path, which is exactly
/// the state this is guarding against.
#[test]
#[ignore = "tier 2: compiles a release cargo workspace"]
fn print_artifact_stdout_is_exactly_one_line() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    project(root, "\"rust\", \"api\"");

    let out = forgedb(
        root,
        &["build", "--print-artifact", "server"],
    );
    ok(&out, "build --print-artifact server");

    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "stdout must carry the path and nothing else, got {} line(s): {stdout:?}",
        lines.len()
    );
    let path = Path::new(lines[0]);
    assert!(
        path.is_absolute() && path.exists(),
        "the printed path must name a real artifact: {}",
        lines[0]
    );
}
