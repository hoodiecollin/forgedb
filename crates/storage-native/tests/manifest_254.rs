//! #254 — two orthogonal version counters on the manifest.
//!
//! Gate 3 scenarios 8 and 9. The manifest carried ONE enforced counter under the
//! wrong name (`format_version` — actually the app's schema-migration serial)
//! plus a vestigial `schema_version` hardcoded to `1` at every write site and
//! never compared. #254 needs a second, genuinely orthogonal counter for the
//! *engine's* byte-format generation, so the two are separated and named:
//!
//! | field | owned by | counts |
//! |---|---|---|
//! | `schema_version` (was `format_version`) | the app's `migrations/` lineage | applied schema migrations |
//! | `engine_version` (new) | ForgeDB's release line | engine byte-format generation |

use forgedb_storage_native::Manifest;
use std::fs;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("forgedb_254_manifest_{name}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d.join("manifest.json")
}

/// Scenario 8 — the ordered rename's whole reason for existing.
///
/// A manifest written by a released ForgeDB holds BOTH keys: the dead
/// `"schema_version": 1` and the real, enforced `"format_version": 6`. Renaming
/// the Rust identifier without first deleting the vestigial field would make new
/// code read the dead `1`, and the generated open-guard would then compare the
/// wrong number — silently opening a data dir six schema migrations out of date,
/// or refusing a correct one.
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
    // The unrelated fields still load — the vestigial key is ignored, not fatal.
    assert_eq!(m.row_count, 3);
    assert_eq!(m.compaction_epoch, 2);
}

/// Scenario 9 — every manifest written before this change baselines to engine
/// generation 1, which is what makes the new counter additive rather than a
/// second format break.
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

/// The on-disk key for the enforced serial does NOT change. The wire name is
/// invisible to users, so renaming it would buy nothing and would strand every
/// existing dir; the Rust identifier is what was wrong.
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
