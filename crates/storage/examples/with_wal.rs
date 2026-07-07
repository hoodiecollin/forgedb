//! Intermediate example for forgedb-storage
//!
//! This example demonstrates using storage with Write-Ahead Log (WAL)
//! for durability and crash recovery.

use forgedb_storage::{
    ColumnMetadata, ColumnType, Database, FsyncPolicy, Transaction, WalEntry, WalValue,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("=== ForgeDB Storage - With WAL ===\n");

    // Create a temporary database directory
    let db_path = PathBuf::from("/tmp/forgedb_storage_wal_example");

    // Clean up if exists
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }

    // Open database with WAL enabled
    println!("--- Creating Database with WAL ---");
    let mut db = Database::open_with_wal(db_path.clone(), FsyncPolicy::Always)?;
    println!("Database opened with WAL at: {:?}", db_path);
    println!("WAL enabled: {}\n", db.has_wal());

    // Set up schema
    println!("--- Setting Up Schema ---");
    let columns = vec![
        ColumnMetadata {
            name: "user_id".to_string(),
            column_type: ColumnType::U64,
            column_index: 0,
            ..Default::default()
        },
        ColumnMetadata {
            name: "username".to_string(),
            column_type: ColumnType::String,
            column_index: 1,
            ..Default::default()
        },
    ];
    db.set_columns(columns);
    db.save_manifest()?;
    println!("Schema: user_id (u64), username (string)\n");

    // Write some entries with WAL
    println!("--- Writing Entries with WAL ---");
    if let Some(wal) = db.wal_mut() {
        // Create an insert operation using the builder method
        let mut fields1 = HashMap::new();
        fields1.insert("user_id".to_string(), WalValue::U64(1001));
        fields1.insert("username".to_string(), WalValue::String("alice".to_string()));

        let insert_entry = WalEntry::insert(
            "User".to_string(),
            uuid::Uuid::new_v4(),
            fields1,
        );

        wal.write(&insert_entry)?;
        println!("Wrote insert to WAL: user_id=1001, username=alice");

        // Create another insert
        let mut fields2 = HashMap::new();
        fields2.insert("user_id".to_string(), WalValue::U64(1002));
        fields2.insert("username".to_string(), WalValue::String("bob".to_string()));

        let insert_entry2 = WalEntry::insert(
            "User".to_string(),
            uuid::Uuid::new_v4(),
            fields2,
        );

        wal.write(&insert_entry2)?;
        println!("Wrote insert to WAL: user_id=1002, username=bob");

        // Flush to ensure durability
        wal.flush()?;
        println!("\n✓ WAL flushed to disk");
    }

    // Check WAL statistics
    println!("\n--- WAL Statistics ---");
    if let Some(wal) = db.wal() {
        let size = wal.size()?;
        let empty = wal.is_empty()?;
        println!("WAL size: {} bytes", size);
        println!("WAL empty: {}", empty);
    }

    // Demonstrate WAL replay
    println!("\n--- WAL Replay ---");
    if let Some(wal) = db.wal_mut() {
        let mut entry_count = 0;
        wal.replay(|entry| {
            entry_count += 1;
            println!("Replayed entry {}: {:?} on {}", entry_count, entry.operation, entry.model_name);
            Ok(())
        })?;
        println!("Total entries replayed: {}", entry_count);
    }

    // Using transactions for atomic operations
    println!("\n--- Using Transactions ---");
    if let Some(wal) = db.wal_mut() {
        // Begin a transaction
        let mut txn = Transaction::begin();
        println!("Started transaction with ID: {}", txn.id());

        // Add operations to transaction
        let mut update_fields1 = HashMap::new();
        update_fields1.insert("username".to_string(), WalValue::String("alice_updated".to_string()));

        let op1 = WalEntry::update(
            "User".to_string(),
            uuid::Uuid::new_v4(),
            update_fields1,
        );
        txn.add_entry(op1)?;

        let mut update_fields2 = HashMap::new();
        update_fields2.insert("username".to_string(), WalValue::String("bob_updated".to_string()));

        let op2 = WalEntry::update(
            "User".to_string(),
            uuid::Uuid::new_v4(),
            update_fields2,
        );
        txn.add_entry(op2)?;

        println!("Added {} operations to transaction", txn.len());

        // Commit the transaction
        txn.commit(wal)?;
        println!("✓ Transaction committed");
    }

    println!("\n--- Final Statistics ---");
    let manifest = db.get_manifest();
    println!("WAL enabled in manifest: {}", manifest.wal_enabled);
    println!("Last checkpoint: {}", manifest.last_checkpoint);

    println!("\n✓ Example completed successfully!");
    println!("Database and WAL files created at: {:?}", db_path);

    Ok(())
}
