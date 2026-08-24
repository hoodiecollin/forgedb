//! #438 — the **stored** half of the enum-reorder defect, witnessed by running
//! two separately generated crates over one data directory.
//!
//! # Why this file cannot be a snapshot test
//!
//! An enum is persisted as a **positional 1-byte discriminant**: `generate_enum`
//! builds `__to_u8`/`__from_u8` with `enumerate()`, so the variant's declaration
//! index *is* the byte, and the variant's name never reaches disk. Reorder two
//! variants and **not one byte on disk changes** — the bytes simply mean
//! something else.
//!
//! Nothing that compares generated code as *strings* can see that. A codegen
//! snapshot would show the two `match` arms swapping, which is the change the
//! author intended and looks correct; the defect is what the swap does to bytes
//! that were written before it. A test asserting on `database.rs` would be
//! asserting on the diff's *shape*. Only a compiled, running pair of crates —
//! one writing, the other reading the same directory — can observe it.
//!
//! # The two tests, and the different jobs they do
//!
//! 1. `a_reordered_enum_silently_remaps_a_stored_row` — the **witness**. It pins
//!    the premise the tier-1 detection in `crates/migrations/tests/diff_tests.rs`
//!    exists to serve.
//!
//!    **It is not a regression test and it does not flip with the #438 fix.**
//!    That fix changes no generated code, so the re-map stays exactly as real as
//!    it was; what changes is that `migrate create` now *reports* it. Saying so
//!    plainly matters, because a test that can never go red on the change it
//!    ships beside invites deletion. What it would go red on is the positional
//!    encoding itself changing — which is the day the classification table in
//!    `crates/migrations/src/types.rs` has to be revisited.
//!
//! 2. `the_json_transport_re_encodes_a_reordered_enum_by_name` — the claim the
//!    **`Auto` classification rests on**. `ChangeEnumVariants` for a reorder is
//!    classified `Auto`, meaning ForgeDB promises the operator that no authoring
//!    is needed. That promise is only good if the transformer's hop really does
//!    repair the row. If this test is red, the reorder row moves from `Auto` to
//!    `Authored` and the table is wrong — which is exactly why it is a test and
//!    not a paragraph.
//!
//! ## What test 2 exercises, and what it deliberately does not
//!
//! It runs the transformer's hop body — **`serde_json::to_value(&row_from)` →
//! (field ops) → `serde_json::from_value::<v_to::Post>()` → `v_to` insert**,
//! `crates/codegen/src/transform.rs` — across two *real generated crates*, with
//! the field-op list empty, which is precisely what a `ChangeEnumVariants` hop
//! emits (`build_model_ops` adds no structural op for it, by design).
//!
//! It does **not** shell out to `forgedb migrate build`. That is a deliberate
//! limit, not an oversight: the transformer scaffold pins its substrate deps from
//! **crates.io** (`forgedb-storage = "0.3"`), so a test that builds one is red for
//! the whole of any cycle carrying a publish gap — which is why
//! `test_migrate_build_reports_the_path_cargo_actually_wrote` is the single
//! `--skip` in `make test-ignored`, and why `tests/ci_gate_test.rs` asserts that
//! skip matches *exactly one* test. A second registry-dependent ignored test
//! cannot be added without breaking that invariant. The mechanism under test is
//! identical either way; only the crate that hosts it differs.
//!
//! # Running it
//!
//! Both tests generate and compile crates, so both are `#[ignore]`d out of the
//! fast suite:
//!
//! ```bash
//! make enum-remap-test      # or:
//! cargo test --test enum_discriminant_remap_test -- --ignored --nocapture
//! ```
//!
//! `cargo test --no-run` will NOT catch a break here: the drivers below are string
//! literals a subprocess compiles at run time, so only *running* this file
//! type-checks them (#381).

// This file uses only `generate_compile_run_in` + `assert_driver_ok`; the rest of
// the shared harness is dead *here* and live in its other consumers.
#[allow(dead_code)]
mod common;

use std::path::PathBuf;

/// The original ordering. `Draft` is discriminant 0.
const SCHEMA_V1: &str = r#"enum Status { Draft, Published, Archived }

Post {
  id: +uuid
  title: string
  status: Status
}
"#;

/// The first two variants swapped, and nothing else. `Published` is now
/// discriminant 0 — so the byte `Draft` wrote reads back as `Published`, and
/// every byte stays in range, so nothing fails.
const SCHEMA_V2: &str = r#"enum Status { Published, Draft, Archived }

Post {
  id: +uuid
  title: string
  status: Status
}
"#;

/// A scratch directory shared by the two crates a test generates. Outside both
/// project dirs, because `assert_driver_ok` removes those on success.
fn shared(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgedb-{tag}-shared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// 1 — the witness
// ---------------------------------------------------------------------------

/// Writes one `Post { status: Draft }` into `argv[1]`.
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

/// Reads `argv[1]` back through a schema whose first two variants are swapped,
/// and reports what the stored byte now means.
///
/// It asserts the value is `Published` — the WRONG one. That is the point: the
/// row was written as `Draft` and no byte on disk has changed. If this ever
/// reads `Draft`, the discriminant has stopped being positional and the whole
/// classification table for `ChangeEnumVariants` needs revisiting.
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

// ---------------------------------------------------------------------------
// 2 — the round-trip the `Auto` classification promises
// ---------------------------------------------------------------------------

/// Writes one `Post { status: Draft }`, then emits the row exactly as the
/// transformer's hop does — `serde_json::to_value(&row)` — into
/// `<data dir>/../row.json`.
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

/// The hop's remaining two steps under the *new* schema: decode the v1 JSON into
/// the v2 typed struct and insert it, then read it back.
///
/// The field-op list a `ChangeEnumVariants` hop emits is EMPTY (see
/// `build_model_ops`), so an identity body is the whole transform — which is
/// what makes this the real mechanism rather than a stand-in for it.
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
