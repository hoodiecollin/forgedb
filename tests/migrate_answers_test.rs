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

const CONFIG: &str = "[project]\nid = \"migrate-answers\"\n\n[generate]\ntargets = [\"all\"]\n";

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
    cmd.args([
        "migrate", "create", description, "--schema", "schema.forge",
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

// ---------------------------------------------------------------------------
// Scenario 11 — an unanswered hop cannot be built
// ---------------------------------------------------------------------------

/// The app's container in the build cache, or `None` if none was reserved.
fn container(dir: &Path) -> Option<PathBuf> {
    let apps = home(dir).join("projects").join("migrate-answers").join("apps");
    let mut found: Vec<PathBuf> = fs::read_dir(&apps)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    found.pop()
}

/// `migrate build` refuses a hop whose required add has no answer — **naming the
/// change**, writing nothing into the cache member, and never invoking cargo.
///
/// The last two are the load-bearing assertions. A refusal that happens after
/// emission leaves a half-written member behind, and a refusal that happens
/// after cargo starts costs a compile to learn something the record already
/// said.
///
/// # Mutation (scenario 14)
///
/// Verified by deleting the `refuse_unanswered_hops(...)` **call site** in
/// `emit_transform_crate` — not the function. This test then goes RED, because
/// the build proceeds. Mutating the function proves the check works; only
/// mutating the call site proves it is reached.
#[test]
fn test_scenario_11_an_unanswered_hop_cannot_be_built() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");

    // The add is recorded PROVABLE (the schema defaults it), so `create`
    // succeeds — and is then hand-stripped, which is what a record looks like
    // when a change was recorded and its answer removed. `create` will not
    // write an unanswered record itself, which is the whole point of step 8;
    // this test is about `build` refusing one that exists.
    fs::write(
        dir.join("schema.forge"),
        "Post {\n  id: +uuid\n  title: string\n  slug: string @default(\"untitled\")\n}\n",
    )
    .unwrap();
    let out = create(dir, "add slug", &[]);
    assert!(out.status.success(), "create failed:\n{}", combined(&out));
    strip_answers(dir);

    let out = forgedb(dir)
        .args([
            "migrate", "build", "--schema", "schema.forge", "--from", "1", "--to", "2",
        ])
        .output()
        .expect("run migrate build");
    let log = combined(&out);

    assert!(!out.status.success(), "the build must refuse:\n{log}");
    assert!(
        log.contains("Post") && log.contains("slug"),
        "the refusal must name the change:\n{log}"
    );
    assert!(
        !log.contains("Compiling") && !log.contains("Finished `dev`"),
        "cargo must never be invoked for a range that cannot be built:\n{log}"
    );
    if let Some(c) = container(dir) {
        let member = c.join("transform-1-2");
        assert!(
            !member.exists(),
            "nothing may be written into the cache member for a refused range: {}",
            member.display()
        );
    }
}

/// Remove every recorded answer — the operator's `answer` **and** the schema's
/// `default_value` — rebuilding the record so its checksum still verifies.
///
/// A hand-stripped record is the fixture scenario 11 asks for: it is what a
/// lineage looks like when a change was recorded and its answer removed.
///
/// It goes through `Migration` rather than through `serde_json::Value` on
/// purpose: a `Value` round-trip reorders the object's keys, and the checksum
/// is computed over the serialized text, so a re-checksummed `Value` verifies
/// against bytes forgedb would never have written.
fn strip_answers(dir: &Path) {
    use forgedb_migrations::{Migration, SchemaChange};
    for path in fs::read_dir(migrations_dir(dir))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let m: Migration = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let changes: Vec<SchemaChange> = m
            .changes
            .iter()
            .cloned()
            .map(|c| match c {
                SchemaChange::AddField {
                    model_name,
                    field_name,
                    field_type,
                    nullable,
                    ..
                } => SchemaChange::AddField {
                    model_name,
                    field_name,
                    field_type,
                    nullable,
                    default_json: None,
                    answer: None,
                },
                other => other,
            })
            .collect();
        let rebuilt = Migration::with_id(
            m.id.clone(),
            m.description.clone(),
            changes,
            m.from_version,
            m.to_version,
        );
        assert!(rebuilt.verify_checksum());
        fs::write(&path, serde_json::to_string_pretty(&rebuilt).unwrap()).unwrap();
    }
}

// ---------------------------------------------------------------------------
// Scenarios 6, 7, 9 — the non-interactive contract, driven through the binary
// ---------------------------------------------------------------------------

/// Non-interactive runs fail at the **FIRST** unprovable change, name it, and
/// write nothing.
///
/// Three assertions, and the second and third are the load-bearing ones.
///
/// * *Names the first.* `Post.slug` specifically, not "2 changes need answers".
/// * *Does not mention the second.* A CI run gets one specific failure, not a
///   batch that reads as ten problems when it is one schema edit.
/// * *Writes nothing.* A refused create that still recorded the migration would
///   leave a lineage whose hop can never be built, and the operator would find
///   out at `migrate build`.
#[test]
fn test_scenario_6_non_interactive_fails_at_the_first_unprovable_change() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n  views: u32\n}\n");

    // A required add AND a type change. `slug` sorts before `views`, so the
    // differ reports it first.
    fs::write(
        dir.join("schema.forge"),
        "Post {\n  id: +uuid\n  title: string\n  views: string\n  slug: string\n}\n",
    )
    .unwrap();
    let out = create(dir, "two problems", &[]);
    let log = combined(&out);

    assert!(!out.status.success(), "must refuse:\n{log}");
    assert!(
        log.contains("Post.slug"),
        "the refusal must name the first change specifically:\n{log}"
    );
    assert!(
        !log.contains("Post.views —") && !log.contains("cannot derive how to re-encode"),
        "only the FIRST change is reported; a batch reads as many problems when it \
         is one schema edit:\n{log}"
    );
    assert!(
        records(dir).is_empty(),
        "a refused create must write NO migration record"
    );
    assert!(
        !dir.join("migrations/schemas/v2.forge").exists(),
        "a refused create must write no versioned schema either"
    );
}

/// `--no-auto` suppresses the prompt, **not** detection.
///
/// A purely provable edit produces byte-identical `changes` with and without
/// it, and both succeed. The flag decides whether an unprovable change stops the
/// run or asks a question — nothing else.
#[test]
fn test_scenario_7_no_auto_suppresses_the_prompt_not_detection() {
    let mut recorded = Vec::new();
    for extra in [vec![], vec!["--no-auto"]] {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();
        fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");
        fs::write(
            dir.join("schema.forge"),
            "Post {\n  id: +uuid\n  title: string\n  summary: string?\n}\n",
        )
        .unwrap();
        let out = create(dir, "add summary", &extra);
        assert!(
            out.status.success(),
            "a provable edit must succeed with {extra:?}:\n{}",
            combined(&out)
        );
        recorded.push(only_record(dir)["changes"].clone());
    }
    assert_eq!(
        recorded[0], recorded[1],
        "`--no-auto` must not change the DIFF, only whether an unprovable change \
         stops the run"
    );
}

/// There is no way to create a migration ForgeDB did not detect.
///
/// The `--auto`-less branch used to write an empty record with `changes: []`
/// and tell the operator to edit it by hand — which is exactly the Rust-authoring
/// default #374 removes, and which lets a record disagree with
/// `migrations/schemas/vN.forge`.
#[test]
fn test_scenario_9_an_unchanged_schema_writes_no_record() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");

    for extra in [vec![], vec!["--no-auto"]] {
        let out = create(dir, "nothing changed", &extra);
        assert!(out.status.success(), "{}", combined(&out));
        assert!(
            combined(&out).contains("No schema changes"),
            "{}",
            combined(&out)
        );
        assert!(
            records(dir).is_empty(),
            "an unchanged schema must write NO record — least of all one with an \
             empty `changes` array"
        );
    }
}

/// A `@default` in the schema answers the question, so nothing is asked and the
/// required add is provable.
///
/// This is direction B's arithmetic showing up at the CLI: the same edit that
/// refuses in scenario 6 succeeds here, non-interactively, because the answer
/// is written down in the `.forge`.
#[test]
fn test_a_schema_default_answers_the_question_before_it_is_asked() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");
    fs::write(
        dir.join("schema.forge"),
        "Post {\n  id: +uuid\n  title: string\n  slug: string @default(\"untitled\")\n}\n",
    )
    .unwrap();
    let out = create(dir, "add slug", &[]);
    assert!(
        out.status.success(),
        "a defaulted required add is provable:\n{}",
        combined(&out)
    );
    let rec = only_record(dir);
    let add = &rec["changes"][0]["AddField"];
    assert_eq!(
        add["default_value"],
        serde_json::json!("\"untitled\""),
        "the resolved default is recorded as the JSON literal both routes write: {rec:#}"
    );
    assert!(
        add.get("answer").is_none(),
        "a schema default is not an operator answer; recording both would be two \
         carriers for one value: {rec:#}"
    );
    assert_eq!(rec["record_version"], 1);
}

// ---------------------------------------------------------------------------
// A hop answered IN THE RECORD needs no file on disk
// ---------------------------------------------------------------------------

/// `migrate build` must not demand a `transform.rs` for a hop whose answer is a
/// constant.
///
/// This is the common case #374 creates, and it was broken: the build read the
/// authored body whenever no *escape language* was recorded, on the stated
/// premise that "no language" means "written before #374". It does not. A
/// current record whose only authored change was answered with a constant or a
/// field copy names no escape language either — and `migrate build` demanded a
/// `transform.rs` that was never scaffolded, for a hop that is fully answered.
/// The fact that actually separates the two is `record_version`.
///
/// The assertion is on the **absence** of that message rather than on a
/// successful build, deliberately: `migrate build` compiles a crate whose
/// substrate deps resolve from crates.io, so a tier-1 test cannot take it to
/// completion. What it CAN prove is that emission got past the point that was
/// wrong, and a message naming `transform.rs` is that point.
#[test]
fn test_a_hop_answered_in_the_record_needs_no_transform_file() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");

    // A required add with no default, answered with a CONSTANT — the shape a
    // terminal `migrate create` produces and the one the bug was reachable
    // through. It is written by hand because the answer comes from a prompt,
    // and the subprocess harness can only ever walk the non-interactive branch.
    fs::write(
        dir.join("schema.forge"),
        "Post {\n  id: +uuid\n  title: string\n  slug: string\n}\n",
    )
    .unwrap();
    write_answered_record(dir);

    let rec = only_record(dir);
    assert_eq!(rec["record_version"], 1, "a current record: {rec:#}");
    assert!(
        rec["changes"][0]["AddField"]["answer"].is_object(),
        "the change is answered IN THE RECORD: {rec:#}"
    );
    assert!(
        !dir.join("migrations")
            .join(rec["id"].as_str().unwrap())
            .join("transform.rs")
            .exists(),
        "no transform was scaffolded, because a constant needs none"
    );

    let out = forgedb(dir)
        .args([
            "migrate", "build", "--schema", "schema.forge", "--from", "1", "--to", "2",
        ])
        .output()
        .expect("run migrate build");
    let log = combined(&out);
    assert!(
        !log.contains("transform.rs"),
        "the build demanded an authored body for a hop that is answered in the \
         record. `record_version`, not the absence of an escape language, is what \
         says a record predates #374:\n{log}"
    );
}

/// Record a v1 -> v2 hop whose one change is an `Authored` required add
/// answered with `Answer::Constant`, plus the v2 schema snapshot the
/// transformer needs.
fn write_answered_record(dir: &Path) {
    use forgedb_migrations::{Answer, Migration, MigrationGenerator, SchemaChange};
    let m = Migration::with_id(
        Migration::next_id(),
        "add slug".to_string(),
        vec![SchemaChange::AddField {
            model_name: "Post".into(),
            field_name: "slug".into(),
            field_type: "string".parse().unwrap(),
            nullable: false,
            default_json: None,
            answer: Some(Answer::Constant {
                json: "\"untitled\"".into(),
            }),
        }],
        1,
        2,
    );
    MigrationGenerator::write_migration(migrations_dir(dir), m).unwrap();
    forgedb_migrations::save_versioned_schema(
        &migrations_dir(dir),
        2,
        &fs::read_to_string(dir.join("schema.forge")).unwrap(),
    )
    .unwrap();
}
