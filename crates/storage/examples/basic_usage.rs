// Example: Basic usage of forgedb-storage
//
// This example demonstrates the core features of the storage crate.

use forgedb_storage::{
    ColumnMetadata, ColumnType, Database, FixedColumn, FsyncPolicy, Tombstones, Transaction,
    VariableColumn, WalValue,
};
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("ForgeDB Storage - Basic Usage Example\n");

    // 1. Creating a Database
    println!("1. Creating a database...");
    let db_path = PathBuf::from("/tmp/forgedb_example/my_database");
    let _ = std::fs::remove_dir_all(&db_path);
    let mut db = Database::open(db_path.clone())?;

    // Define schema
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
    db.save_manifest()?;
    println!("   ✓ Database created at {:?}", db_path);

    // 2. Working with Fixed-Size Columns
    println!("\n2. Working with fixed-size columns (u64)...");
    let id_col_path = db.fixed_column_path(0);
    let mut id_col = FixedColumn::new(id_col_path, 8)?;

    id_col.append_u64(1001)?;
    id_col.append_u64(1002)?;
    id_col.append_u64(1003)?;

    println!("   ✓ Appended 3 IDs");
    println!("   ✓ ID at index 0: {}", id_col.read_u64(0)?);
    println!("   ✓ ID at index 1: {}", id_col.read_u64(1)?);
    println!("   ✓ ID at index 2: {}", id_col.read_u64(2)?);
    println!("   ✓ Total rows: {}", id_col.len());

    // 3. Working with Variable-Length Columns
    println!("\n3. Working with variable-length columns (strings)...");
    let email_data_path = db.variable_data_path(0);
    let email_offsets_path = db.variable_offsets_path(0);
    let mut email_col = VariableColumn::new(email_data_path, email_offsets_path)?;

    email_col.append_string("alice@example.com")?;
    email_col.append_string("bob@example.com")?;
    email_col.append_string("charlie@example.com")?;

    println!("   ✓ Appended 3 emails");
    println!("   ✓ Email at index 0: {}", email_col.read_string(0)?);
    println!("   ✓ Email at index 1: {}", email_col.read_string(1)?);
    println!("   ✓ Email at index 2: {}", email_col.read_string(2)?);
    println!("   ✓ Total rows: {}", email_col.len());

    // 4. Using Tombstones for Soft Deletes
    println!("\n4. Using tombstones for soft deletes...");
    let tombstones_path = db.tombstones_path();
    let mut tombstones = Tombstones::new(tombstones_path)?;

    tombstones.append(false)?; // Row 0: active
    tombstones.append(true)?; // Row 1: deleted
    tombstones.append(false)?; // Row 2: active

    println!("   ✓ Row 0 deleted: {}", tombstones.is_deleted(0)?);
    println!("   ✓ Row 1 deleted: {}", tombstones.is_deleted(1)?);
    println!("   ✓ Row 2 deleted: {}", tombstones.is_deleted(2)?);

    // 5. WAL Integration
    println!("\n5. WAL integration...");
    let wal_db_path = PathBuf::from("/tmp/forgedb_example/wal_database");
    let _ = std::fs::remove_dir_all(&wal_db_path);
    let mut wal_db = Database::open_with_wal(wal_db_path.clone(), FsyncPolicy::Always)?;

    // Create a transaction and add entries
    let mut txn = Transaction::begin();
    let user_id = uuid::Uuid::new_v4();
    
    // Create a WalEntry for insert
    let mut fields = std::collections::HashMap::new();
    fields.insert("id".to_string(), WalValue::U64(1001));
    fields.insert(
        "email".to_string(),
        WalValue::String("user@example.com".to_string()),
    );
    
    let insert_entry = forgedb_storage::WalEntry::insert("User".to_string(), user_id, fields);
    txn.add_entry(insert_entry)?;

    // Commit transaction to WAL
    if let Some(wal) = wal_db.wal_mut() {
        txn.commit(wal)?;
        println!("   ✓ Transaction written and committed to WAL");
    }

    // 6. Persistence Test
    println!("\n6. Testing persistence...");
    drop(id_col); // Close the column

    let mut id_col_reopened = FixedColumn::new(db.fixed_column_path(0), 8)?;
    println!("   ✓ Reopened column file");
    println!(
        "   ✓ Data persisted: {} rows",
        id_col_reopened.len()
    );
    println!(
        "   ✓ First ID after reopening: {}",
        id_col_reopened.read_u64(0)?
    );

    println!("\n✅ All examples completed successfully!");
    println!("\nDatabase files created at:");
    println!("   - {}", db_path.display());
    println!("   - {}", wal_db_path.display());

    Ok(())
}
