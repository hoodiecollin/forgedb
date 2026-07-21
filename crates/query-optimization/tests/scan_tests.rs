use forgedb_query_optimization::scan::*;

#[test]
fn test_scan_u64_eq() {
    let data: Vec<u64> = (0..100).collect();
    let result = ColumnScan::scan_u64(&data, ScanFilter::Eq(42), None);

    assert_eq!(result.matching_rows, vec![42]);
    assert_eq!(result.rows_scanned, 100);
    assert_eq!(result.early_termination, false);
}

#[test]
fn test_scan_u64_gt() {
    let data: Vec<u64> = (0..100).collect();
    let result = ColumnScan::scan_u64(&data, ScanFilter::Gt(95), None);

    assert_eq!(result.matching_rows, vec![96, 97, 98, 99]);
    assert_eq!(result.rows_scanned, 100);
}

#[test]
fn test_scan_u64_range() {
    let data: Vec<u64> = (0..100).collect();
    let result = ColumnScan::scan_u64(&data, ScanFilter::Range(10, 15), None);

    assert_eq!(result.matching_rows, vec![10, 11, 12, 13, 14, 15]);
}

#[test]
fn test_scan_u64_early_termination() {
    let data: Vec<u64> = (0..10000).collect();
    let result = ColumnScan::scan_u64(&data, ScanFilter::Gte(0), Some(10));

    assert_eq!(result.matching_rows.len(), 10);
    assert_eq!(result.early_termination, true);
    assert!(result.rows_scanned < 10000); // Should not scan all rows
}

#[test]
fn test_scan_u64_batch_processing() {
    // Test with data larger than BATCH_SIZE
    let data: Vec<u64> = (0..5000).collect();
    let result = ColumnScan::scan_u64(&data, ScanFilter::Eq(4999), None);

    assert_eq!(result.matching_rows, vec![4999]);
    assert_eq!(result.rows_scanned, 5000);
}

#[test]
fn test_scan_i64_negative() {
    let data: Vec<i64> = (-50..50).collect();
    let result = ColumnScan::scan_i64(&data, ScanFilter::Lt(0), None);

    assert_eq!(result.matching_rows.len(), 50);
    assert_eq!(result.rows_scanned, 100);
}

#[test]
fn test_scan_f64() {
    let data: Vec<f64> = (0..100).map(|x| x as f64 * 0.5).collect();
    let result = ColumnScan::scan_f64(&data, ScanFilter::Gte(25.0), Some(5));

    assert_eq!(result.matching_rows.len(), 5);
    assert_eq!(result.early_termination, true);
}
