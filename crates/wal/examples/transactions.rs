//! Intermediate example for forgedb-wal
//!
//! This example demonstrates using transactions for atomic operations,
//! including commit and rollback scenarios.

use forgedb_wal::{
    FsyncPolicy, Transaction, TransactionReplay, WalEntry, WalManager, WalOperation, WalValue,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("=== ForgeDB WAL - Transactions ===\n");

    // Create a temporary WAL file
    let wal_path = PathBuf::from("/tmp/forgedb_wal_transactions_example.log");

    // Clean up if exists
    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
    }

    // Create a new WAL with periodic fsync (more performant)
    println!("--- Creating WAL ---");
    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Never)?;
    println!("WAL opened with FsyncPolicy::Never (manual flush required)\n");

    // Example 1: Successful transaction
    println!("--- Example 1: Successful Transaction ---");
    let mut txn1 = Transaction::begin();
    println!("Transaction {} started", txn1.id());

    // Add multiple operations to the transaction
    let mut fields1 = HashMap::new();
    fields1.insert("account".to_string(), WalValue::String("Alice".to_string()));
    fields1.insert("amount".to_string(), WalValue::F64(100.0));

    let entry1 = WalEntry::insert(
        "Transaction".to_string(),
        uuid::Uuid::new_v4(),
        fields1,
    );
    txn1.add_entry(entry1)?;

    let mut fields2 = HashMap::new();
    fields2.insert("account".to_string(), WalValue::String("Bob".to_string()));
    fields2.insert("amount".to_string(), WalValue::F64(-100.0));

    let entry2 = WalEntry::insert(
        "Transaction".to_string(),
        uuid::Uuid::new_v4(),
        fields2,
    );
    txn1.add_entry(entry2)?;

    println!("Added {} operations to transaction", txn1.len());

    // Commit the transaction
    txn1.commit(&mut wal)?;
    println!("✓ Transaction committed\n");

    // Example 2: Rolled back transaction
    println!("--- Example 2: Rolled Back Transaction ---");
    let mut txn2 = Transaction::begin();
    println!("Transaction {} started", txn2.id());

    let mut fields3 = HashMap::new();
    fields3.insert("account".to_string(), WalValue::String("Charlie".to_string()));
    fields3.insert("amount".to_string(), WalValue::F64(50.0));

    let entry3 = WalEntry::insert(
        "Transaction".to_string(),
        uuid::Uuid::new_v4(),
        fields3,
    );
    txn2.add_entry(entry3)?;

    println!("Added {} operation to transaction", txn2.len());

    // Rollback instead of commit (e.g., validation failed)
    txn2.rollback(&mut wal)?;
    println!("✓ Transaction rolled back\n");

    // Example 3: Multiple independent operations outside transactions
    println!("--- Example 3: Direct Operations ---");
    let mut fields4 = HashMap::new();
    fields4.insert("status".to_string(), WalValue::String("completed".to_string()));

    let direct_entry = WalEntry::update(
        "Job".to_string(),
        uuid::Uuid::new_v4(),
        fields4,
    );

    wal.write(&direct_entry)?;
    println!("✓ Wrote direct update operation\n");

    // Manually flush since we're using FsyncPolicy::Never
    wal.flush()?;
    println!("✓ WAL flushed to disk\n");

    // Replay with transaction tracking
    println!("--- Replaying with Transaction Tracking ---");
    let mut replay = TransactionReplay::new();
    let mut entry_count = 0;

    wal.replay(|entry| {
        entry_count += 1;
        
        // Track transaction boundaries
        replay.process_entry(entry);

        match &entry.operation {
            WalOperation::BeginTransaction { txn_id } => {
                println!("  Begin Transaction {}", txn_id);
            }
            WalOperation::CommitTransaction { txn_id } => {
                println!("  Commit Transaction {}", txn_id);
            }
            WalOperation::RollbackTransaction { txn_id } => {
                println!("  Rollback Transaction {}", txn_id);
            }
            _ => {
                println!("  Operation: {:?} on {}", entry.operation, entry.model_name);
            }
        }

        Ok(())
    })?;

    println!("\n--- Transaction Statistics ---");
    println!("Total entries: {}", entry_count);
    println!("Committed transactions: {:?}", replay.committed_transactions());
    println!("Rolled back transactions: {:?}", replay.rolledback_transactions());
    println!("Active (incomplete) transactions: {:?}", replay.active_transactions());

    // WAL rotation example
    println!("\n--- WAL Rotation ---");
    let archived_path = wal.rotate()?;
    println!("✓ WAL rotated. Archive created at: {:?}", archived_path);
    println!("WAL is now empty: {}", wal.is_empty()?);

    println!("\n✓ Example completed successfully!");

    Ok(())
}
