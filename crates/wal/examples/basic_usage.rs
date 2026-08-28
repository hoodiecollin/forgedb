use forgedb_wal::{FsyncPolicy, WalEntry, WalManager, WalOperation};
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("=== ForgeDB WAL - Basic Usage ===\n");

    let wal_path = PathBuf::from("/tmp/forgedb_wal_basic_example.log");

    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
    }

    println!("--- Creating WAL ---");
    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always)?;
    println!("WAL opened at: {:?}\n", wal_path);

    println!("--- Writing Raw Entries ---");

    let entry_a = WalEntry::raw("Post", b"serialized post row #1".to_vec());
    wal.write(&entry_a)?;
    println!("Wrote Raw entry for model 'Post'");

    let entry_b = WalEntry::raw("Comment", b"serialized comment row #1".to_vec());
    wal.write(&entry_b)?;
    println!("Wrote Raw entry for model 'Comment'");

    wal.flush()?;
    println!("\nAll entries flushed to disk");

    println!("\n--- WAL Statistics ---");
    println!("WAL size:  {} bytes", wal.size()?);
    println!("WAL empty: {}", wal.is_empty()?);

    println!("\n--- Replaying WAL Entries (crash recovery) ---");
    let mut count = 0;
    wal.replay(|entry| {
        count += 1;
        match &entry.operation {
            WalOperation::Raw { payload } => {
                println!(
                    "Entry {}: model='{}', payload_len={}",
                    count,
                    entry.model_name,
                    payload.len()
                );
            }
        }
        Ok(())
    })?;
    println!("\nReplayed {} entries", count);

    println!("\n--- WAL Rotation ---");
    let archive = wal.rotate()?;
    println!("Archived WAL at: {:?}", archive);
    println!("Fresh WAL is empty: {}", wal.is_empty()?);

    println!("\nExample completed successfully!");
    Ok(())
}
