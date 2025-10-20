use forgedb_storage::*;
use std::fs;

#[test]
fn test_fixed_column_u64() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    // Append values
    col.append_u64(42).unwrap();
    col.append_u64(100).unwrap();
    col.append_u64(999).unwrap();

    // Read values
    assert_eq!(col.read_u64(0).unwrap(), 42);
    assert_eq!(col.read_u64(1).unwrap(), 100);
    assert_eq!(col.read_u64(2).unwrap(), 999);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_variable");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    // Append strings
    col.append_string("hello").unwrap();
    col.append_string("world").unwrap();
    col.append_string("test@example.com").unwrap();

    // Read strings
    assert_eq!(col.read_string(0).unwrap(), "hello");
    assert_eq!(col.read_string(1).unwrap(), "world");
    assert_eq!(col.read_string(2).unwrap(), "test@example.com");
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_tombstones");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("tombstones.bin");
    let mut tombstones = Tombstones::new(path).unwrap();

    tombstones.append(false).unwrap();
    tombstones.append(true).unwrap();
    tombstones.append(false).unwrap();

    assert_eq!(tombstones.is_deleted(0).unwrap(), false);
    assert_eq!(tombstones.is_deleted(1).unwrap(), true);
    assert_eq!(tombstones.is_deleted(2).unwrap(), false);
    assert_eq!(tombstones.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_database_manifest() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_manifest");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut db = Database::open(temp_dir.clone()).unwrap();

    // Set up columns
    let columns = vec![
        ColumnMetadata {
            name: "id".to_string(),
            column_type: ColumnType::U64,
            column_index: 0,
        },
        ColumnMetadata {
            name: "email".to_string(),
            column_type: ColumnType::String,
            column_index: 0,
        },
    ];

    db.set_columns(columns);
    db.update_row_count(42);
    db.save_manifest().unwrap();

    // Reopen and verify
    let db2 = Database::open(temp_dir.clone()).unwrap();
    assert_eq!(db2.get_manifest().row_count, 42);
    assert_eq!(db2.get_manifest().columns.len(), 2);
    assert_eq!(db2.get_manifest().columns[0].name, "id");
    assert_eq!(db2.get_manifest().columns[1].name, "email");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_out_of_bounds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    col.append_u64(42).unwrap();

    // Reading beyond bounds should error
    let result = col.read_u64(1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_persistence() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_fixed_persist");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u64.bin");

    // Write data
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(100).unwrap();
        col.append_u64(200).unwrap();
        col.append_u64(300).unwrap();
    }

    // Reopen and verify
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        assert_eq!(col.len(), 3);
        assert_eq!(col.read_u64(0).unwrap(), 100);
        assert_eq!(col.read_u64(1).unwrap(), 200);
        assert_eq!(col.read_u64(2).unwrap(), 300);
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_empty_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_empty_string");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    col.append_string("").unwrap();
    col.append_string("hello").unwrap();
    col.append_string("").unwrap();

    assert_eq!(col.read_string(0).unwrap(), "");
    assert_eq!(col.read_string(1).unwrap(), "hello");
    assert_eq!(col.read_string(2).unwrap(), "");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_large_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_large_string");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    // Test with 1KB string
    let large_string = "x".repeat(1024);
    col.append_string(&large_string).unwrap();
    col.append_string("small").unwrap();

    assert_eq!(col.read_string(0).unwrap(), large_string);
    assert_eq!(col.read_string(1).unwrap(), "small");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_persistence() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_var_persist");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");

    // Write data
    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        col.append_string("first").unwrap();
        col.append_string("second").unwrap();
        col.append_string("third").unwrap();
    }

    // Reopen and verify
    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        assert_eq!(col.len(), 3);
        assert_eq!(col.read_string(0).unwrap(), "first");
        assert_eq!(col.read_string(1).unwrap(), "second");
        assert_eq!(col.read_string(2).unwrap(), "third");

        // Append more after reopening
        col.append_string("fourth").unwrap();
    }

    // Verify new data persisted
    {
        let mut col = VariableColumn::new(data_path, offsets_path).unwrap();
        assert_eq!(col.len(), 4);
        assert_eq!(col.read_string(3).unwrap(), "fourth");
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_out_of_bounds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_var_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    col.append_string("test").unwrap();

    // Reading beyond bounds should error
    let result = col.read_string(1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones_out_of_bounds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_tomb_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("tombstones.bin");
    let mut tombstones = Tombstones::new(path).unwrap();

    tombstones.append(false).unwrap();

    // Reading beyond bounds should error
    let result = tombstones.is_deleted(1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_database_empty() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_empty");
    let _ = fs::remove_dir_all(&temp_dir);

    let db = Database::open(temp_dir.clone()).unwrap();

    // Empty database should have default manifest
    assert_eq!(db.get_manifest().row_count, 0);
    assert_eq!(db.get_manifest().columns.len(), 0);
    assert_eq!(db.get_manifest().schema_version, 1);

    fs::remove_dir_all(&temp_dir).unwrap();
}
