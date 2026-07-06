//! Basic usage example for forgedb-storage
//!
//! This example demonstrates creating a database, working with columns,
//! and basic CRUD operations on storage.

use forgedb_storage::{ColumnMetadata, ColumnType, Database, FixedColumn, VariableColumn};
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("=== ForgeDB Storage - Basic Usage ===\n");

    // Create a temporary database directory
    let db_path = PathBuf::from("/tmp/forgedb_storage_basic_example");
    
    // Clean up if exists
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }

    // Open a new database
    println!("--- Creating Database ---");
    let mut db = Database::open(db_path.clone())?;
    println!("Database opened at: {:?}\n", db_path);

    // Set up schema metadata
    println!("--- Setting Up Schema ---");
    let columns = vec![
        ColumnMetadata {
            name: "id".to_string(),
            column_type: ColumnType::U64,
            column_index: 0,
        },
        ColumnMetadata {
            name: "email".to_string(),
            column_type: ColumnType::String,
            column_index: 1,
        },
    ];
    db.set_columns(columns);
    db.save_manifest()?;
    println!("Schema defined: id (u64), email (string)\n");

    // Working with fixed-size column (u64)
    println!("--- Working with Fixed Column (u64) ---");
    let mut id_column = FixedColumn::new(db.fixed_column_path_typed(0, &ColumnType::U64), 8)?;
    
    // Insert some IDs
    id_column.append_u64(1001)?;
    id_column.append_u64(1002)?;
    id_column.append_u64(1003)?;
    println!("Inserted 3 IDs");

    // Read them back (read_u64 now takes &self — concurrent reads are safe)
    for i in 0..id_column.len() {
        let id = id_column.read_u64(i)?;
        println!("ID at index {}: {}", i, id);
    }
    // Explicit flush at commit boundary
    id_column.flush()?;
    println!();

    // Working with variable-length column (string)
    println!("--- Working with Variable Column (string) ---");
    let mut email_column = VariableColumn::new(
        db.variable_data_path(1),
        db.variable_offsets_path(1),
    )?;

    // Insert some emails
    email_column.append_string("alice@example.com")?;
    email_column.append_string("bob@example.com")?;
    email_column.append_string("charlie@example.com")?;
    println!("Inserted 3 emails");

    // Read them back (read_string now takes &self)
    for i in 0..email_column.len() {
        let email = email_column.read_string(i)?;
        println!("Email at index {}: {}", i, email);
    }
    // Explicit flush at commit boundary
    email_column.flush()?;
    println!();

    // Update row count and save manifest
    db.update_row_count(3);
    db.save_manifest()?;
    
    println!("--- Database Statistics ---");
    let manifest = db.get_manifest();
    println!("Total rows: {}", manifest.row_count);
    println!("Total columns: {}", manifest.columns.len());
    println!("Schema version: {}", manifest.schema_version);

    println!("\n✓ Example completed successfully!");
    println!("Database files created at: {:?}", db_path);

    Ok(())
}
