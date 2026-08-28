#[allow(dead_code)]
mod common;

use std::path::PathBuf;

const SCHEMA_V1: &str = r#"enum Status { Draft, Published, Archived }

Post {
  id: +uuid
  title: string
  status: Status
}
"#;

const SCHEMA_V2: &str = r#"enum Status { Published, Draft, Archived }

Post {
  id: +uuid
  title: string
  status: Status
}
"#;

fn shared(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgedb-{tag}-shared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const WRITE_DRAFT: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let mut db = Database::open_at(dir);
    db.create_post(Post {
        id: "22222222-2222-2222-2222-222222222222".parse().expect("parse uuid"),
        title: "hello".to_string(),
        status: Status::Draft,
    })
    .expect("create post");
    println!("wrote status=Draft (discriminant 0 under this schema)");
}
"##;

const READ_BACK: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let db = Database::open_at(dir);
    let rows = db.post.all();
    assert_eq!(rows.len(), 1, "expected the one row the writer stored, got {:?}", rows);
    let got = rows[0].status;
    println!("read back status={:?}", got);
    assert_eq!(
        got,
        Status::Published,
        "the stored byte did not re-map. Either the writer never ran, or the \
         discriminant is no longer keyed by declaration position — in which case \
         #438's classification of a reorder is wrong and must be revisited."
    );
    println!("WITNESSED: a row written as Draft reads back as Published, with no \
              error, no warning, and no byte on disk changed");
}
"##;

#[test]
#[ignore = "generates and compiles two crates; run with --ignored (see `make enum-remap-test`)"]
fn a_reordered_enum_silently_remaps_a_stored_row() {
    let shared = shared("enumremap");
    let data = shared.join("data");

    let (out, proj) =
        common::generate_compile_run_in("enumremapwriter", SCHEMA_V1, WRITE_DRAFT, Some(&data));
    common::assert_driver_ok(&out, &proj, "the v1 writer failed");

    let (out, proj) =
        common::generate_compile_run_in("enumremapreader", SCHEMA_V2, READ_BACK, Some(&data));
    common::assert_driver_ok(
        &out,
        &proj,
        "the v2 reader did not observe the re-mapped discriminant",
    );

    let _ = std::fs::remove_dir_all(&shared);
}

const EMIT_ROW_JSON: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let out = dir.parent().expect("data dir has a parent").join("row.json");
    let mut db = Database::open_at(dir);
    db.create_post(Post {
        id: "22222222-2222-2222-2222-222222222222".parse().expect("parse uuid"),
        title: "hello".to_string(),
        status: Status::Draft,
    })
    .expect("create post");

    // This is the hop's first step verbatim (transform.rs): every row is read
    // through the v_from typed struct and serialized to a JSON value.
    let row = db.post.all().into_iter().next().expect("the row just written");
    let json = serde_json::to_value(&row).expect("serialize the v1 row");
    println!("v1 row as JSON: {}", json);
    assert_eq!(
        json["status"],
        serde_json::Value::String("Draft".to_string()),
        "an enum must cross the transformer's JSON boundary as its NAME — the \
         whole `Auto` classification of a reorder depends on it"
    );
    std::fs::write(&out, serde_json::to_string(&json).unwrap()).expect("write row.json");
}
"##;

const DECODE_AND_INSERT: &str = r##"mod database;
use database::*;
mod api;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let src = dir.parent().expect("data dir has a parent").join("row.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&src).expect("read row.json"))
            .expect("parse row.json");

    // No field ops: `build_model_ops` emits none for a ChangeEnumVariants hop.
    let record: Post = serde_json::from_value(json).expect("decode the v1 row at v2");

    let mut db = Database::open_at(dir);
    db.create_post(record).expect("insert the migrated row");

    let rows = db.post.all();
    assert_eq!(rows.len(), 1, "expected exactly the migrated row, got {:?}", rows);
    println!("migrated row reads back status={:?}", rows[0].status);
    assert_eq!(
        rows[0].status,
        Status::Draft,
        "the JSON name round-trip did NOT re-encode the reordered enum. If this \
         is what happens, a reorder is not `Auto` — it is `Authored`, and \
         SchemaChange::enum_verdict in crates/migrations/src/types.rs is wrong."
    );
    println!("ROUND TRIP OK: a row written under the old ordering reads back as \
              Draft under the new one, with an identity hop body");
}
"##;

#[test]
#[ignore = "generates and compiles two crates; run with --ignored (see `make enum-remap-test`)"]
fn the_json_transport_re_encodes_a_reordered_enum_by_name() {
    let shared = shared("enumroundtrip");

    let (out, proj) = common::generate_compile_run_in(
        "enumtripsrc",
        SCHEMA_V1,
        EMIT_ROW_JSON,
        Some(&shared.join("src-data")),
    );
    common::assert_driver_ok(&out, &proj, "the v1 emitter failed");

    let (out, proj) = common::generate_compile_run_in(
        "enumtripdst",
        SCHEMA_V2,
        DECODE_AND_INSERT,
        Some(&shared.join("dest-data")),
    );
    common::assert_driver_ok(
        &out,
        &proj,
        "the v2 hop body did not re-encode the reordered enum",
    );

    let _ = std::fs::remove_dir_all(&shared);
}
