/// Transaction support for WAL
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::entry::WalEntry;
use crate::writer::WalWriter;

/// Transaction ID type
pub type TransactionId = u64;

/// Global transaction ID counter
static NEXT_TXN_ID: AtomicU64 = AtomicU64::new(1);

/// Transaction state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    Active,
    Committed,
    RolledBack,
}

/// Transaction context
///
/// A transaction groups multiple operations together with atomic commit.
/// All operations in a transaction are either committed together or rolled back.
pub struct Transaction {
    id: TransactionId,
    state: TransactionState,
    entries: Vec<WalEntry>,
}

impl Transaction {
    /// Begin a new transaction
    pub fn begin() -> Self {
        let id = NEXT_TXN_ID.fetch_add(1, Ordering::SeqCst);

        Transaction {
            id,
            state: TransactionState::Active,
            entries: Vec::new(),
        }
    }

    /// Get the transaction ID
    pub fn id(&self) -> TransactionId {
        self.id
    }

    /// Get the transaction state
    pub fn state(&self) -> TransactionState {
        self.state
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.state == TransactionState::Active
    }

    /// Add an entry to the transaction
    pub fn add_entry(&mut self, entry: WalEntry) -> io::Result<()> {
        if !self.is_active() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot add entry to inactive transaction",
            ));
        }

        self.entries.push(entry);
        Ok(())
    }

    /// Commit the transaction to WAL
    pub fn commit(mut self, wal: &mut crate::WalManager) -> io::Result<()> {
        if !self.is_active() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Transaction is not active",
            ));
        }

        // Write BEGIN marker
        let begin_entry = WalEntry::begin_transaction(self.id);
        wal.write(&begin_entry)?;

        // Write all entries
        for entry in &self.entries {
            wal.write(entry)?;
        }

        // Write COMMIT marker
        let commit_entry = WalEntry::commit_transaction(self.id);
        wal.write(&commit_entry)?;

        // Ensure all data is flushed to disk
        wal.flush()?;

        self.state = TransactionState::Committed;
        Ok(())
    }

    /// Rollback the transaction
    pub fn rollback(mut self, wal: &mut crate::WalManager) -> io::Result<()> {
        if !self.is_active() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Transaction is not active",
            ));
        }

        // Write BEGIN marker (if any entries were added)
        if !self.entries.is_empty() {
            let begin_entry = WalEntry::begin_transaction(self.id);
            wal.write(&begin_entry)?;

            // Write ROLLBACK marker
            let rollback_entry = WalEntry::rollback_transaction(self.id);
            wal.write(&rollback_entry)?;

            wal.flush()?;
        }

        self.state = TransactionState::RolledBack;
        Ok(())
    }

    /// Get the number of entries in the transaction
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if transaction is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Transaction replay state tracker
///
/// Used during WAL replay to track which transactions were committed
pub struct TransactionReplay {
    committed_txns: std::collections::HashSet<TransactionId>,
    rolledback_txns: std::collections::HashSet<TransactionId>,
    active_txns: std::collections::HashMap<TransactionId, Vec<WalEntry>>,
}

impl TransactionReplay {
    /// Create a new transaction replay tracker
    pub fn new() -> Self {
        TransactionReplay {
            committed_txns: std::collections::HashSet::new(),
            rolledback_txns: std::collections::HashSet::new(),
            active_txns: std::collections::HashMap::new(),
        }
    }

    /// Process a WAL entry during replay
    pub fn process_entry(&mut self, entry: &WalEntry) {
        match &entry.operation {
            crate::entry::WalOperation::BeginTransaction { txn_id } => {
                self.active_txns.insert(*txn_id, Vec::new());
            }
            crate::entry::WalOperation::CommitTransaction { txn_id } => {
                self.committed_txns.insert(*txn_id);
                self.active_txns.remove(txn_id);
            }
            crate::entry::WalOperation::RollbackTransaction { txn_id } => {
                self.rolledback_txns.insert(*txn_id);
                self.active_txns.remove(txn_id);
            }
            _ => {
                // Regular operation - add to current transaction if one is active
                // This is handled by the caller checking if we're in a transaction
            }
        }
    }

    /// Check if a transaction was committed
    pub fn is_committed(&self, txn_id: TransactionId) -> bool {
        self.committed_txns.contains(&txn_id)
    }

    /// Check if a transaction was rolled back
    pub fn is_rolledback(&self, txn_id: TransactionId) -> bool {
        self.rolledback_txns.contains(&txn_id)
    }

    /// Check if a transaction is still active (incomplete)
    pub fn is_active(&self, txn_id: TransactionId) -> bool {
        self.active_txns.contains_key(&txn_id)
    }

    /// Get all committed transaction IDs
    pub fn committed_transactions(&self) -> &std::collections::HashSet<TransactionId> {
        &self.committed_txns
    }

    /// Get all rolled back transaction IDs
    pub fn rolledback_transactions(&self) -> &std::collections::HashSet<TransactionId> {
        &self.rolledback_txns
    }

    /// Get all active (incomplete) transaction IDs
    pub fn active_transactions(&self) -> Vec<TransactionId> {
        self.active_txns.keys().copied().collect()
    }
}

impl Default for TransactionReplay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::WalValue;
    use crate::writer::FsyncPolicy;
    use std::collections::HashMap;

    #[test]
    fn test_transaction_basic() {
        let mut txn = Transaction::begin();
        assert!(txn.is_active());
        assert_eq!(txn.len(), 0);

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        txn.add_entry(entry).unwrap();
        assert_eq!(txn.len(), 1);
    }

    #[test]
    fn test_transaction_commit() {
        let temp_dir = std::env::temp_dir().join("sinkdb_txn_commit");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut wal = crate::WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        let mut txn = Transaction::begin();
        let txn_id = txn.id();

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        txn.add_entry(entry).unwrap();
        txn.commit(&mut wal).unwrap();

        // Verify WAL contains BEGIN, entry, and COMMIT
        let entries = wal.replay(|_| Ok(())).unwrap();

        assert_eq!(entries.len(), 3); // BEGIN + INSERT + COMMIT

        match &entries[0].operation {
            crate::entry::WalOperation::BeginTransaction { txn_id: id } => {
                assert_eq!(*id, txn_id);
            }
            _ => panic!("Expected BeginTransaction"),
        }

        match &entries[1].operation {
            crate::entry::WalOperation::Insert { .. } => {}
            _ => panic!("Expected Insert"),
        }

        match &entries[2].operation {
            crate::entry::WalOperation::CommitTransaction { txn_id: id } => {
                assert_eq!(*id, txn_id);
            }
            _ => panic!("Expected CommitTransaction"),
        }

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_transaction_rollback() {
        let temp_dir = std::env::temp_dir().join("sinkdb_txn_rollback");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut wal = crate::WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        let mut txn = Transaction::begin();
        let txn_id = txn.id();

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        txn.add_entry(entry).unwrap();
        txn.rollback(&mut wal).unwrap();

        // Verify WAL contains BEGIN and ROLLBACK
        let entries = wal.replay(|_| Ok(())).unwrap();

        assert_eq!(entries.len(), 2); // BEGIN + ROLLBACK

        match &entries[0].operation {
            crate::entry::WalOperation::BeginTransaction { txn_id: id } => {
                assert_eq!(*id, txn_id);
            }
            _ => panic!("Expected BeginTransaction"),
        }

        match &entries[1].operation {
            crate::entry::WalOperation::RollbackTransaction { txn_id: id } => {
                assert_eq!(*id, txn_id);
            }
            _ => panic!("Expected RollbackTransaction"),
        }

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_transaction_replay() {
        let mut replay = TransactionReplay::new();

        let txn_id = 1;

        // Process BEGIN
        let begin = WalEntry::begin_transaction(txn_id);
        replay.process_entry(&begin);
        assert!(replay.is_active(txn_id));

        // Process COMMIT
        let commit = WalEntry::commit_transaction(txn_id);
        replay.process_entry(&commit);
        assert!(replay.is_committed(txn_id));
        assert!(!replay.is_active(txn_id));
    }

    #[test]
    fn test_transaction_empty_rollback() {
        let temp_dir = std::env::temp_dir().join("sinkdb_txn_empty_rollback");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut wal = crate::WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        let txn = Transaction::begin();
        assert!(txn.is_empty());

        txn.rollback(&mut wal).unwrap();

        // Empty transaction rollback should not write anything to WAL
        let entries = wal.replay(|_| Ok(())).unwrap();
        assert_eq!(entries.len(), 0);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
}
