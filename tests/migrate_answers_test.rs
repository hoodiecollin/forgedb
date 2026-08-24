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

// ---------------------------------------------------------------------------
// Scenario 4 — one default, two routes, identical rows (TIER 2)
// ---------------------------------------------------------------------------

/// v1: three `Post` rows, no `status`.
const S4_V1: &str = "Post {\n  id: +uuid\n  title: string\n}\n";

/// v2: `status` added, required, with a `@default`.
const S4_V2: &str =
    "Post {\n  id: +uuid\n  title: string\n  status: string @default(\"pending\")\n}\n";

/// Writes three rows under v1 into `argv[1]`, and emits each row's JSON — the
/// transformer hop's first step, verbatim — into `<data>/../rows.json`.
const S4_WRITE_V1: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let out = dir.parent().expect("data dir has a parent").join("rows.json");
    let mut db = Database::open_at(dir);
    for (i, title) in ["alpha", "beta", "gamma"].iter().enumerate() {
        db.create_post(Post {
            id: format!("00000000-0000-0000-0000-00000000000{}", i + 1)
                .parse()
                .expect("parse uuid"),
            title: title.to_string(),
        })
        .expect("create post");
    }
    let rows: Vec<serde_json::Value> = db
        .post
        .all()
        .iter()
        .map(|r| serde_json::to_value(r).expect("serialize v1 row"))
        .collect();
    assert_eq!(rows.len(), 3);
    std::fs::write(&out, serde_json::to_string(&rows).unwrap()).expect("write rows.json");
    println!("wrote 3 v1 rows with no `status` column");
}
"##;

/// ROUTE A — the reopen backfill. Opens the **v1 data dir** with the v2 schema.
/// `recover_from_wal` finds `status_col` short of the tombstone anchor and
/// backfills it.
const S4_REOPEN: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let db = Database::open_at(dir);
    let mut rows = db.post.all();
    rows.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(rows.len(), 3, "expected the three v1 rows, got {:?}", rows);
    for r in &rows {
        println!("reopen route: {} -> status={:?}", r.title, r.status);
        assert_eq!(
            r.status, "pending",
            "the reopen backfill wrote {:?} instead of the schema's @default. \
             That is finding 4: the same edit, different data by route.",
            r.status
        );
    }
    println!("ROUTE A OK: alpha,beta,gamma all read status=pending");
}
"##;

/// ROUTE B — the transformer hop, over a FRESH dir. Reads the v1 row JSON,
/// applies the one structural op a `@default` add emits (`field_adds`, whose
/// literal `default_fill` produced), decodes into the v2 struct and inserts.
///
/// The `"pending"` literal below is the one
/// `crates/codegen/tests/default_fill_test.rs::the_scenario_4_fixtures_literal_is_what_default_fill_produces`
/// pins — this driver is a `const &str` a subprocess compiles, so it cannot
/// compute it.
const S4_TRANSFORM: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let src = dir.parent().expect("data dir has a parent").join("rows.json");
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&src).expect("read rows.json"))
            .expect("parse rows.json");
    assert_eq!(rows.len(), 3);

    let mut db = Database::open_at(dir);
    for mut j in rows {
        // `build_model_ops` emits exactly this for a defaulted add: insert the
        // recorded JSON literal under the field's name, then decode.
        if let Some(o) = j.as_object_mut() {
            o.insert("status".to_string(), serde_json::from_str("\"pending\"").unwrap());
        }
        let rec: Post = serde_json::from_value(j).expect("decode v1 row at v2");
        db.create_post(rec).expect("insert migrated row");
    }

    let mut got = db.post.all();
    got.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(got.len(), 3);
    for r in &got {
        println!("transform route: {} -> status={:?}", r.title, r.status);
        assert_eq!(r.status, "pending");
    }
    println!("ROUTE B OK: alpha,beta,gamma all read status=pending");
}
"##;

/// Both routes over the same schema edit produce the same rows.
///
/// **Compiles and RUNS the generated code, and asserts the row values.** Nothing
/// that compares generated code as strings can see this defect: each route was
/// individually well-formed and only their *disagreement* corrupted — `""` in
/// one dir and `"pending"` in the other, decided by which command the operator
/// happened to run.
#[test]
#[ignore = "generates and compiles three crates; run with --ignored"]
fn scenario_4_one_default_two_routes_identical_rows() {
    // Only `generate_compile_run_in` + `assert_driver_ok` are used here; the
    // rest of the shared harness is dead in this file and live in its others.
    #[allow(dead_code)]
    #[path = "common/mod.rs"]
    mod common;

    let shared = std::env::temp_dir().join(format!("forgedb-s4-{}", std::process::id()));
    let _ = fs::remove_dir_all(&shared);
    fs::create_dir_all(&shared).unwrap();
    let v1_data = shared.join("v1-data");

    let (out, proj) = common::generate_compile_run_in("s4writer", S4_V1, S4_WRITE_V1, Some(&v1_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    // Route A reopens the SAME directory the v1 writer left behind.
    let (out, proj) = common::generate_compile_run_in("s4reopen", S4_V2, S4_REOPEN, Some(&v1_data));
    common::assert_driver_ok(&out, &proj, "the reopen backfill did not honour @default");

    // Route B builds a fresh destination, as the transformer does.
    let (out, proj) = common::generate_compile_run_in(
        "s4transform",
        S4_V2,
        S4_TRANSFORM,
        Some(&shared.join("v2-data")),
    );
    common::assert_driver_ok(&out, &proj, "the transformer route did not honour @default");

    let _ = fs::remove_dir_all(&shared);
}
