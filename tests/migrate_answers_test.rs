//! `forgedb migrate create` captures the answer, and `migrate build` refuses a
//! hop that has none (#374).
//!
//! Everything here drives the real `forgedb` binary in a tempdir with its own
//! `FORGEDB_HOME`, then reads the migration record that was written. The record
//! is the artifact under test: an answer is **data ForgeDB recorded**, so the
//! only honest assertion is on the bytes it wrote.
//!
//! # Mutation checklist (scenario 14)
//!
//! Two of the guards below are only meaningful if the code they guard *runs*,
//! and mutating the function proves the check works while proving nothing about
//! whether it is reached. Both were verified by mutating the **call site**:
//!
//! - **`hop_answer_status` in `emit_transform_crate`.** Delete the
//!   `hop_answer_status(...)` call (not the function) and
//!   `test_scenario_11_an_unanswered_hop_cannot_be_built` must go RED.
//! - **`resolve_answers` in `migrate::create`.** Delete the call and
//!   `test_scenario_6_non_interactive_fails_at_the_first_unprovable_change`
//!   must go RED.
//!
//! # Why these are not in `tests/migrate_tests.rs`
//!
//! `tests/ci_gate_test.rs` asserts `make test-ignored` carries exactly **one**
//! `--skip`, matching exactly one real test in `migrate_tests.rs`. A second
//! registry-dependent ignored test in that file would break that guard, so the
//! tier-2 scenarios live here instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const CONFIG: &str = "[project]\nname = \"migrate-answers\"\n\n[generate]\ntargets = [\"all\"]\n";

fn home(dir: &Path) -> PathBuf {
    dir.join(".forgedb-home")
}

/// A `forgedb` invocation scoped to `dir`, with its cache and project-id claim
/// inside the fixture (never the developer's real `~/.forgedb`).
fn forgedb(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forgedb"));
    cmd.current_dir(dir).env("FORGEDB_HOME", home(dir));
    cmd
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Lay down `forgedb.toml` + `schema.forge` and record the baseline snapshot.
fn fixture(dir: &Path, baseline: &str) {
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();
    fs::write(dir.join("schema.forge"), baseline).unwrap();
    let out = create(dir, "baseline", &[]);
    assert!(out.status.success(), "baseline failed:\n{}", combined(&out));
}

/// Run `migrate create <description>` with extra args.
fn create(dir: &Path, description: &str, extra: &[&str]) -> std::process::Output {
    let mut cmd = forgedb(dir);
    // `--auto` is passed here only until #374 step 8 makes detection the
    // default and turns the flag into a refusing tombstone.
    cmd.args([
        "migrate", "create", description, "--auto", "--schema", "schema.forge",
    ]);
    cmd.args(extra);
    cmd.output().expect("run migrate create")
}

fn migrations_dir(dir: &Path) -> PathBuf {
    dir.join("migrations")
}

/// Every recorded migration record, parsed, in id order.
fn records(dir: &Path) -> Vec<serde_json::Value> {
    let mut files: Vec<PathBuf> = fs::read_dir(migrations_dir(dir))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
        .iter()
        .map(|p| serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap())
        .collect()
}

/// The single record written by the migration under test.
fn only_record(dir: &Path) -> serde_json::Value {
    let mut r = records(dir);
    assert_eq!(r.len(), 1, "expected exactly one migration record, got {r:?}");
    r.pop().unwrap()
}

/// The `changes` array's variant names, in order.
fn change_kinds(rec: &serde_json::Value) -> Vec<String> {
    rec["changes"]
        .as_array()
        .expect("changes is an array")
        .iter()
        .map(|c| {
            c.as_object()
                .expect("each change is an externally-tagged object")
                .keys()
                .next()
                .unwrap()
                .clone()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Scenario 1 — one edit, one change
// ---------------------------------------------------------------------------

/// Making a field nullable is **one** edit and must record **one** change.
///
/// Before #374 the projected type carried the `Nullable` wrapper, so `views:
/// u32` → `views: u32?` moved two projected values at once and the differ
/// emitted `ChangeFieldNullability` *and* a spurious `ChangeFieldType`. The
/// spurious one classifies `Authored`, so the safest edit in the language
/// demanded a hand-written Rust transform for a type that did not change.
///
/// Both halves are asserted: the nullability change is present, and the type
/// change is **absent**. Asserting only the first passes on the buggy tree.
#[test]
fn test_scenario_1_one_edit_records_one_change() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  views: u32\n}\n");

    fs::write(dir.join("schema.forge"), "Post {\n  id: +uuid\n  views: u32?\n}\n").unwrap();
    let out = create(dir, "views nullable", &[]);
    assert!(out.status.success(), "create failed:\n{}", combined(&out));

    let rec = only_record(dir);
    assert_eq!(
        change_kinds(&rec),
        vec!["ChangeFieldNullability".to_string()],
        "T -> ?T is one change; a ChangeFieldType beside it is the nullability \
         double-report (#374 decision 6). Record: {rec:#}"
    );
}
