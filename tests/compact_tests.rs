use forgedb::commands::compact::format_bytes;

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_bytes(1536), "1.50 KB");
}

/// #105: the offline `forgedb compact` / `vacuum` write path is DEPRECATED and
/// removed — it is unsafe on data written by the generated mutation surface (it
/// can resurrect deleted rows).  Both commands now mutate nothing and return the
/// deprecation error (non-zero exit) pointing to in-process `Database::compact()`.
/// Read-only stats/analyze are unaffected.
#[test]
fn test_offline_compact_is_deprecated_and_mutates_nothing() {
    use forgedb::commands::compact::{compact, vacuum, CompactOptions, VacuumOptions};

    let tmp = std::env::temp_dir().join(format!("fdb_compact_dep_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Seed a sentinel file so we can assert nothing was touched.
    let sentinel = tmp.join("model_a").join("tombstones.bin");
    std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    std::fs::write(&sentinel, b"\x01\x00\x01").unwrap();

    let err = compact(CompactOptions {
        data_dir: tmp.clone(),
        model: None,
        all: false,
        threshold: None,
    })
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("#105")
            && msg.to_lowercase().contains("removed")
            && msg.to_lowercase().contains("resurrect")
            && msg.contains("Database::compact()"),
        "compact must return the #105 deprecation guidance: {err}"
    );

    let err = vacuum(VacuumOptions {
        data_dir: tmp.clone(),
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("#105"),
        "vacuum must return the #105 deprecation guidance: {err}"
    );

    // Neither command may mutate on-disk data.
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"\x01\x00\x01",
        "deprecated compaction commands must not touch data files"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
