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

