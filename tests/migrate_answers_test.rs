use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const CONFIG: &str = "[project]\nid = \"migrate-answers\"\n\n[generate]\ntargets = [\"all\"]\n";

fn home(dir: &Path) -> PathBuf {
    dir.join(".forgedb-home")
}

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

fn fixture(dir: &Path, baseline: &str) {
    fs::write(dir.join("forgedb.toml"), CONFIG).unwrap();
    fs::write(dir.join("schema.forge"), baseline).unwrap();
    let out = create(dir, "baseline", &[]);
    assert!(out.status.success(), "baseline failed:\n{}", combined(&out));
}

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

fn only_record(dir: &Path) -> serde_json::Value {
    let mut r = records(dir);
    assert_eq!(r.len(), 1, "expected exactly one migration record, got {r:?}");
    r.pop().unwrap()
}

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

const S4_V1: &str = "Post {\n  id: +uuid\n  title: string\n}\n";

const S4_V2: &str =
    "Post {\n  id: +uuid\n  title: string\n  status: string @default(\"pending\")\n}\n";

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

#[test]
#[ignore = "generates and compiles three crates; run with --ignored"]
fn scenario_4_one_default_two_routes_identical_rows() {
    #[allow(dead_code)]
    #[path = "common/mod.rs"]
    mod common;

    let shared = std::env::temp_dir().join(format!("forgedb-s4-{}", std::process::id()));
    let _ = fs::remove_dir_all(&shared);
    fs::create_dir_all(&shared).unwrap();
    let v1_data = shared.join("v1-data");

    let (out, proj) = common::generate_compile_run_in("s4writer", S4_V1, S4_WRITE_V1, Some(&v1_data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    let (out, proj) = common::generate_compile_run_in("s4reopen", S4_V2, S4_REOPEN, Some(&v1_data));
    common::assert_driver_ok(&out, &proj, "the reopen backfill did not honour @default");

    let (out, proj) = common::generate_compile_run_in(
        "s4transform",
        S4_V2,
        S4_TRANSFORM,
        Some(&shared.join("v2-data")),
    );
    common::assert_driver_ok(&out, &proj, "the transformer route did not honour @default");

    let _ = fs::remove_dir_all(&shared);
}

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

#[test]
fn test_scenario_11_an_unanswered_hop_cannot_be_built() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");

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

#[test]
fn test_scenario_6_non_interactive_fails_at_the_first_unprovable_change() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n  views: u32\n}\n");

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

#[test]
fn test_a_hop_answered_in_the_record_needs_no_transform_file() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fixture(dir, "Post {\n  id: +uuid\n  title: string\n}\n");

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
