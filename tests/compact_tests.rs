use forgedb::commands::compact::format_bytes;

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_bytes(1536), "1.50 KB");
}

/// #105: the offline `forgedb compact` / `vacuum` write path is unsafe on data
/// written by the generated mutation surface (it can resurrect deleted rows), so
/// it must REFUSE without `--force`.  Read-only stats/analyze are unaffected.
#[test]
fn test_offline_compact_refuses_without_force() {
    use forgedb::commands::compact::{compact, vacuum, CompactOptions, VacuumOptions};

    let tmp = std::env::temp_dir().join(format!("fdb_compact_guard_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let err = compact(CompactOptions {
        data_dir: tmp.clone(),
        model: None,
        all: false,
        threshold: None,
        force: false,
    })
    .unwrap_err();
    assert!(
        format!("{err}").contains("#105") && format!("{err}").to_lowercase().contains("resurrect"),
        "compact must refuse with the #105 explanation: {err}"
    );

    let err = vacuum(VacuumOptions {
        data_dir: tmp.clone(),
        force: false,
    })
    .unwrap_err();
    assert!(format!("{err}").contains("#105"), "vacuum must refuse: {err}");

    let _ = std::fs::remove_dir_all(&tmp);
}
