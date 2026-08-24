//! A migration record written **before** #374 still loads, and its checksum
//! still verifies.
//!
//! This is the quiet case, and it is the one that would have shipped broken.
//! Every lineage already committed carries `record_version: 0`, no `answer` on
//! any change, `default_value` as the add's key, and a `field_type` written as
//! `format!("{:?}", FieldType::String)`. Nothing about those records is wrong;
//! they simply predate the format.
//!
//! A migration record's checksum is the thing that says nobody edited it, and
//! it is computed over the record's **own JSON**. So any change to how a record
//! serializes — an added field, a renamed key — makes every already-committed
//! record re-serialize differently, fail `verify_checksum`, and be **refused at
//! load** with a message about a file the user never touched. Three decisions
//! in `types.rs` exist for that reason and are asserted here:
//!
//! * `record_version` is `skip_serializing_if` zero,
//! * `answer` is `skip_serializing_if` `None`,
//! * `default_json`'s on-disk key is frozen as `default_value`.
//!
//! The fixture below is a golden vector: it is the literal text pre-#374
//! forgedb wrote, checksum included.

use forgedb_migrations::{Migration, MigrationGenerator, SchemaChange};

/// A real pre-#374 record, byte for byte.
const LEGACY: &str = r#"{"id":"20260101000000","description":"legacy hop","created_at":"2026-01-01T00:00:00Z","changes":[{"AddField":{"model_name":"Post","field_name":"slug","field_type":"String","nullable":true,"default_value":null}}],"checksum":"fnv1a64:d507a4ed03b9655c","from_version":1,"to_version":2}"#;

#[test]
fn a_pre_374_record_loads_and_its_checksum_still_verifies() {
    let m: Migration = serde_json::from_str(LEGACY).expect("a pre-#374 record must still parse");
    assert_eq!(m.record_version, 0, "an absent record_version reads as legacy");
    assert!(
        m.verify_checksum(),
        "a pre-#374 record must still verify. It does NOT if a field was added \
         without `skip_serializing_if`, or if `default_value`'s wire key moved — \
         and the user sees a checksum error about a file they never touched."
    );
    assert!(
        m.unanswered().is_empty(),
        "this legacy add is nullable, so it is provable and wants no answer"
    );
}

/// And it survives the real loader, which refuses a checksum mismatch.
#[test]
fn the_loader_accepts_a_pre_374_record() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("20260101000000_legacy_hop.json");
    std::fs::write(&path, LEGACY).unwrap();
    let m = MigrationGenerator::load_migration(&path)
        .expect("the loader must accept a pre-#374 record");
    assert_eq!(m.id, "20260101000000");
}

/// The three serialization decisions, asserted directly.
///
/// The golden vector above would catch any of them breaking, but it would catch
/// them as "the checksum is wrong", which does not say which. These name them.
#[test]
fn a_legacy_shaped_record_re_serializes_to_exactly_the_legacy_bytes() {
    let m: Migration = serde_json::from_str(LEGACY).unwrap();
    let out = serde_json::to_string(&m).unwrap();
    assert_eq!(
        out, LEGACY,
        "a pre-#374 record must round-trip byte for byte"
    );
    assert!(!out.contains("record_version"), "a zero record_version is skipped");
    assert!(!out.contains("\"answer\""), "a None answer is skipped");
    assert!(out.contains("\"default_value\""), "the add's wire key is frozen");
}

/// A record written *now* carries the new format explicitly.
#[test]
fn a_record_written_now_stamps_its_format() {
    let m = Migration::with_id(
        "20260808000000".to_string(),
        "new hop".to_string(),
        vec![SchemaChange::RemoveField {
            model_name: "Post".to_string(),
            field_name: "slug".to_string(),
        }],
        1,
        2,
    );
    assert_eq!(m.record_version, forgedb_migrations::RECORD_VERSION);
    assert!(m.verify_checksum());
    let out = serde_json::to_string(&m).unwrap();
    assert!(
        out.contains("\"record_version\":1"),
        "a current record states its format: {out}"
    );
}

/// The id is allocated **before** the record exists, so a scaffold written at
/// `migrations/<id>/` can be hashed into an answer the record's own checksum
/// then covers (#374).
///
/// Getting this ordering wrong produces a record whose checksum does not cover
/// its answers — which loads fine, and silently permits editing them afterwards.
#[test]
fn the_id_can_be_allocated_before_the_record() {
    let id = Migration::next_id();
    assert_eq!(id.len(), 14, "a timestamp id: {id}");
    let m = Migration::with_id(id.clone(), "x".into(), vec![], 1, 2);
    assert_eq!(m.id, id);
    assert!(m.verify_checksum());
}

/// The checksum covers the answer.
///
/// Not incidental: it is the whole reason `next_id`/`with_id` are split. An
/// answer the checksum did not cover could be edited after the fact and the
/// record would still read as intact.
#[test]
fn the_checksum_covers_the_answer() {
    use forgedb_migrations::Answer;
    let change = |answer: Option<Answer>| SchemaChange::ChangeFieldType {
        model_name: "Post".into(),
        field_name: "views".into(),
        old_type: "u32".parse().unwrap(),
        new_type: "string".parse().unwrap(),
        answer,
    };
    let m = Migration::with_id(
        "20260808000001".into(),
        "x".into(),
        vec![change(Some(Answer::Constant {
            json: "\"0\"".into(),
        }))],
        1,
        2,
    );
    assert!(m.verify_checksum());

    // Tamper with the answer only.
    let mut tampered = m.clone();
    tampered.changes = vec![change(Some(Answer::Constant {
        json: "\"999\"".into(),
    }))];
    assert!(
        !tampered.verify_checksum(),
        "editing an answer must break the checksum, or the record's integrity \
         claim does not extend to the thing #374 added"
    );
}
