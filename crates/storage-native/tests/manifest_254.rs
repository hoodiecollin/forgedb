use forgedb_storage_native::Manifest;
use std::fs;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("forgedb_254_manifest_{name}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d.join("manifest.json")
}

#[test]
fn a_legacy_manifest_reads_the_enforced_serial_not_the_dead_constant() {
    let path = tmp("legacy_both_keys");
    fs::write(
        &path,
        r#"{"schema_version":1,"row_count":3,"columns":[],"wal_enabled":false,
            "last_checkpoint":0,"compaction_epoch":2,"format_version":6}"#,
    )
    .unwrap();

    let m = Manifest::load_from(&path).unwrap();
    assert_eq!(
        m.schema_version, 6,
        "`schema_version` must resolve to the ENFORCED serial (on-disk key \
         `format_version`), never to the vestigial constant"
    );
    assert_eq!(m.row_count, 3);
    assert_eq!(m.compaction_epoch, 2);
}

#[test]
fn a_manifest_with_no_engine_version_baselines_to_one() {
    let path = tmp("no_engine_key");
    fs::write(
        &path,
        r#"{"schema_version":1,"row_count":0,"columns":[],"format_version":1}"#,
    )
    .unwrap();

    let m = Manifest::load_from(&path).unwrap();
    assert_eq!(m.engine_version, 1);
}

#[test]
fn the_on_disk_key_for_the_serial_is_still_format_version() {
    let path = tmp("wire_name");
    let m = Manifest {
        schema_version: 6,
        engine_version: 2,
        row_count: 0,
        columns: vec![],
        wal_enabled: false,
        last_checkpoint: 0,
        compaction_epoch: 0,
        row_anchor: None,
        auto_sequences: Default::default(),
    };
    m.save_to(&path).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json.get("format_version").and_then(|v| v.as_u64()), Some(6));
    assert_eq!(json.get("engine_version").and_then(|v| v.as_u64()), Some(2));
    assert!(
        json.get("schema_version").is_none(),
        "the vestigial key is gone from what we write; it is only tolerated on read"
    );

    let back = Manifest::load_from(&path).unwrap();
    assert_eq!(back.schema_version, 6);
    assert_eq!(back.engine_version, 2);
}
