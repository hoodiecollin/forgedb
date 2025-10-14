pub mod background;
pub mod compactor;
pub mod stats;
pub mod types;

pub use background::BackgroundCompactor;
pub use compactor::Compactor;
pub use stats::StatsCollector;
pub use types::*;

/// Maintenance API for database operations
pub struct MaintenanceApi {
    compactor: Compactor,
    stats_collector: StatsCollector,
}

impl MaintenanceApi {
    pub fn new<P: AsRef<std::path::Path>>(data_dir: P, config: CompactionConfig) -> Self {
        let data_dir = data_dir.as_ref();
        Self {
            compactor: Compactor::new(data_dir, config),
            stats_collector: StatsCollector::new(data_dir),
        }
    }

    /// Collect database statistics
    pub fn stats(&self) -> Result<DatabaseStats, String> {
        self.stats_collector.collect_database_stats()
    }

    /// Collect statistics for a specific model
    pub fn model_stats(&self, model_name: &str) -> Result<ModelStats, String> {
        self.stats_collector.collect_model_stats(model_name)
    }

    /// Manually trigger compaction for a specific model
    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String> {
        self.compactor.compact_model(model_name)
    }

    /// Compact all models
    pub fn compact_all(&self) -> Result<Vec<CompactionResult>, String> {
        self.compactor.compact_all()
    }

    /// Compact only models that need it (based on threshold)
    pub fn compact_needed(&self) -> Result<Vec<CompactionResult>, String> {
        self.compactor.compact_needed()
    }

    /// Vacuum operation - alias for compact (removes dead space)
    pub fn vacuum(&self) -> Result<Vec<CompactionResult>, String> {
        self.compact_all()
    }

    /// Analyze operation - collect and return statistics
    pub fn analyze(&self) -> Result<DatabaseStats, String> {
        self.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_maintenance_api() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();
        let model_dir = data_dir.join("User");

        // Create test model
        fs::create_dir_all(model_dir.join("fixed")).unwrap();

        let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
        tombstone_file.write_all(&[0b00000010]).unwrap();

        let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
        id_file.write_all(&[0u8; 48]).unwrap();

        let api = MaintenanceApi::new(data_dir, CompactionConfig::default());

        // Test stats collection
        let db_stats = api.stats().unwrap();
        assert_eq!(db_stats.models.len(), 1);

        let model_stats = api.model_stats("User").unwrap();
        assert_eq!(model_stats.name, "User");
        assert_eq!(model_stats.total_rows, 3);
        assert_eq!(model_stats.deleted_rows, 1);

        // Test compaction
        let result = api.compact_model("User").unwrap();
        assert!(result.success);
        assert!(result.bytes_reclaimed > 0);

        // Verify after compaction
        let model_stats_after = api.model_stats("User").unwrap();
        assert_eq!(model_stats_after.active_rows, 2);
        assert_eq!(model_stats_after.deleted_rows, 0);
    }

    #[test]
    fn test_vacuum_and_analyze() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();
        let model_dir = data_dir.join("User");

        fs::create_dir_all(model_dir.join("fixed")).unwrap();

        let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
        tombstone_file.write_all(&[0b01010101]).unwrap();

        let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
        id_file.write_all(&[0u8; 128]).unwrap();

        let api = MaintenanceApi::new(data_dir, CompactionConfig::default());

        // Analyze before vacuum
        let stats_before = api.analyze().unwrap();
        assert!(stats_before.dead_bytes > 0);

        // Vacuum
        let results = api.vacuum().unwrap();
        assert!(!results.is_empty());
        assert!(results[0].success);

        // Analyze after vacuum
        let stats_after = api.analyze().unwrap();
        assert_eq!(stats_after.dead_bytes, 0);
    }
}
