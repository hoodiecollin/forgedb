//! Basic usage example for forgedb-wal
//!
//! This example demonstrates creating a Write-Ahead Log (WAL),
//! writing entries, and replaying them for crash recovery.

use forgedb_wal::{FsyncPolicy, WalEntry, WalManager, WalValue};
use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("=== ForgeDB WAL - Basic Usage ===\n");

    // Create a temporary WAL file
    let wal_path = PathBuf::from("/tmp/forgedb_wal_basic_example.log");

    // Clean up if exists
    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
    }

    // Create a new WAL with automatic fsync after every write
    println!("--- Creating WAL ---");
    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always)?;
    println!("WAL opened at: {:?}\n", wal_path);

    // Write some Insert operations
    println!("--- Writing Insert Operations ---");
    
    // Insert operation for a user record
    let mut user_fields = HashMap::new();
    user_fields.insert("id".to_string(), WalValue::U64(1));
    user_fields.insert("name".to_string(), WalValue::String("Alice".to_string()));
    user_fields.insert("age".to_string(), WalValue::U64(30));
    user_fields.insert("active".to_string(), WalValue::Bool(true));

    let insert_entry = WalEntry::insert(
        "User".to_string(),
        uuid::Uuid::new_v4(),
        user_fields,
    );

    wal.write(&insert_entry)?;
    println!("✓ Wrote insert for User: Alice");

    // Insert another record
    let mut user_fields2 = HashMap::new();
    user_fields2.insert("id".to_string(), WalValue::U64(2));
    user_fields2.insert("name".to_string(), WalValue::String("Bob".to_string()));
    user_fields2.insert("age".to_string(), WalValue::U64(25));
    user_fields2.insert("active".to_string(), WalValue::Bool(true));

    let insert_entry2 = WalEntry::insert(
        "User".to_string(),
        uuid::Uuid::new_v4(),
        user_fields2,
    );

    wal.write(&insert_entry2)?;
    println!("✓ Wrote insert for User: Bob");

    // Write an Update operation
    println!("\n--- Writing Update Operation ---");
    let mut update_fields = HashMap::new();
    update_fields.insert("age".to_string(), WalValue::U64(31));
    update_fields.insert("active".to_string(), WalValue::Bool(false));

    let update_entry = WalEntry::update(
        "User".to_string(),
        uuid::Uuid::new_v4(),
        update_fields,
    );

    wal.write(&update_entry)?;
    println!("✓ Wrote update for User");

    // Write a Delete operation
    println!("\n--- Writing Delete Operation ---");
    let delete_entry = WalEntry::delete("User".to_string(), uuid::Uuid::new_v4());
    wal.write(&delete_entry)?;
    println!("✓ Wrote delete for User");

    // Flush to disk
    wal.flush()?;
    println!("\n✓ All entries flushed to disk");

    // Check WAL statistics
    println!("\n--- WAL Statistics ---");
    let size = wal.size()?;
    let empty = wal.is_empty()?;
    println!("WAL size: {} bytes", size);
    println!("WAL empty: {}", empty);

    // Replay the WAL (simulating crash recovery)
    println!("\n--- Replaying WAL Entries ---");
    let mut entry_count = 0;
    wal.replay(|entry| {
        entry_count += 1;
        println!(
            "Entry {}: {:?} on model '{}'",
            entry_count, entry.operation, entry.model_name
        );
        Ok(())
    })?;

    println!("\n✓ Successfully replayed {} entries", entry_count);
    println!("\n✓ Example completed successfully!");

    Ok(())
}
