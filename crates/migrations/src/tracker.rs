use crate::types::{MigrationRecord, MigrationState};
use std::fs;
use std::path::{Path, PathBuf};

/// Tracks applied migrations
pub struct MigrationTracker {
    state_file: PathBuf,
    state: MigrationState,
}

impl MigrationTracker {
    /// Create a new migration tracker
    pub fn new<P: AsRef<Path>>(migrations_dir: P) -> Result<Self, String> {
        let state_file = migrations_dir.as_ref().join(".migration_state.json");
        let state = if state_file.exists() {
            Self::load_state(&state_file)?
        } else {
            MigrationState::default()
        };

        Ok(MigrationTracker { state_file, state })
    }

    /// Load migration state from file
    fn load_state<P: AsRef<Path>>(path: P) -> Result<MigrationState, String> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read migration state: {}", e))?;

        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse migration state: {}", e))
    }

    /// Save migration state to file
    fn save_state(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.state)
            .map_err(|e| format!("Failed to serialize migration state: {}", e))?;

        fs::write(&self.state_file, json)
            .map_err(|e| format!("Failed to write migration state: {}", e))
    }

    /// Check if a migration has been applied
    pub fn is_applied(&self, migration_id: &str) -> bool {
        self.state.is_applied(migration_id)
    }

    /// Mark a migration as applied
    pub fn mark_applied(&mut self, migration_id: String, checksum: String) -> Result<(), String> {
        if self.is_applied(&migration_id) {
            return Err(format!("Migration {} is already applied", migration_id));
        }

        self.state.add_migration(migration_id, checksum);
        self.save_state()
    }

    /// Mark last migration as rolled back
    pub fn mark_rolled_back(&mut self) -> Result<Option<MigrationRecord>, String> {
        let record = self.state.remove_last_migration();
        self.save_state()?;
        Ok(record)
    }

    /// Get list of applied migrations
    pub fn applied_migrations(&self) -> &[MigrationRecord] {
        &self.state.applied_migrations
    }

    /// Get the last applied migration
    pub fn last_migration(&self) -> Option<&MigrationRecord> {
        self.state.last_migration()
    }

    /// Get pending migrations (those not yet applied)
    pub fn pending_migrations(&self, all_migrations: &[String]) -> Vec<String> {
        all_migrations
            .iter()
            .filter(|id| !self.is_applied(id))
            .cloned()
            .collect()
    }

    /// Verify checksum of applied migration
    pub fn verify_checksum(
        &self,
        migration_id: &str,
        expected_checksum: &str,
    ) -> Result<(), String> {
        for record in &self.state.applied_migrations {
            if record.migration_id == migration_id {
                if record.checksum != expected_checksum {
                    return Err(format!(
                        "Checksum mismatch for migration {}: expected {}, got {}",
                        migration_id, expected_checksum, record.checksum
                    ));
                }
                return Ok(());
            }
        }
        Err(format!(
            "Migration {} not found in applied migrations",
            migration_id
        ))
    }

    /// Get migration status summary
    pub fn status_summary(&self, total_migrations: usize) -> String {
        let applied = self.state.applied_migrations.len();
        let pending = total_migrations.saturating_sub(applied);

        format!(
            "Applied: {} | Pending: {} | Total: {}",
            applied, pending, total_migrations
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_migration_tracker() {
        let temp_dir = TempDir::new().unwrap();
        let mut tracker = MigrationTracker::new(temp_dir.path()).unwrap();

        assert!(!tracker.is_applied("20241014000000"));

        tracker
            .mark_applied("20241014000000".to_string(), "abc123".to_string())
            .unwrap();
        assert!(tracker.is_applied("20241014000000"));

        // Create a new tracker instance to verify persistence
        let tracker2 = MigrationTracker::new(temp_dir.path()).unwrap();
        assert!(tracker2.is_applied("20241014000000"));
    }

    #[test]
    fn test_rollback() {
        let temp_dir = TempDir::new().unwrap();
        let mut tracker = MigrationTracker::new(temp_dir.path()).unwrap();

        tracker
            .mark_applied("20241014000000".to_string(), "abc123".to_string())
            .unwrap();
        tracker
            .mark_applied("20241014000001".to_string(), "def456".to_string())
            .unwrap();

        assert_eq!(tracker.applied_migrations().len(), 2);

        let rolled_back = tracker.mark_rolled_back().unwrap();
        assert!(rolled_back.is_some());
        assert_eq!(rolled_back.unwrap().migration_id, "20241014000001");
        assert_eq!(tracker.applied_migrations().len(), 1);
    }

    #[test]
    fn test_pending_migrations() {
        let temp_dir = TempDir::new().unwrap();
        let mut tracker = MigrationTracker::new(temp_dir.path()).unwrap();

        tracker
            .mark_applied("20241014000000".to_string(), "abc123".to_string())
            .unwrap();

        let all = vec![
            "20241014000000".to_string(),
            "20241014000001".to_string(),
            "20241014000002".to_string(),
        ];

        let pending = tracker.pending_migrations(&all);
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0], "20241014000001");
        assert_eq!(pending[1], "20241014000002");
    }
}
