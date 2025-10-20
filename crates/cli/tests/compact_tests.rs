use forgedb_cli::commands::compact::format_bytes;

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_bytes(1536), "1.50 KB");
}
