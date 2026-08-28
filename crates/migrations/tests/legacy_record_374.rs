use forgedb_migrations::{Migration, MigrationGenerator, SchemaChange};

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

#[test]
fn the_loader_accepts_a_pre_374_record() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("20260101000000_legacy_hop.json");
    std::fs::write(&path, LEGACY).unwrap();
    let m = MigrationGenerator::load_migration(&path)
        .expect("the loader must accept a pre-#374 record");
    assert_eq!(m.id, "20260101000000");
}

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

#[test]
fn the_id_can_be_allocated_before_the_record() {
    let id = Migration::next_id();
    assert_eq!(id.len(), 14, "a timestamp id: {id}");
    let m = Migration::with_id(id.clone(), "x".into(), vec![], 1, 2);
    assert_eq!(m.id, id);
    assert!(m.verify_checksum());
}

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
