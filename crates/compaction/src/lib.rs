#![doc = include_str!("../docs/crate.md")]
#[doc = include_str!("../docs/background.md")]
pub mod background;
#[doc = include_str!("../docs/compactor.md")]
pub mod compactor;
#[doc = include_str!("../docs/stats.md")]
pub mod stats;
#[doc = include_str!("../docs/types.md")]
pub mod types;

pub use background::BackgroundCompactor;
pub use compactor::Compactor;
pub use stats::StatsCollector;
pub use types::*;

#[doc = include_str!("../docs/MaintenanceApi.md")]
pub struct MaintenanceApi {
    compactor: Compactor,
    stats_collector: StatsCollector,
}

impl MaintenanceApi {
    #[doc = include_str!("../docs/MaintenanceApi.new.md")]
    pub fn new<P: AsRef<std::path::Path>>(data_dir: P, config: CompactionConfig) -> Self {
        let data_dir = data_dir.as_ref();
        Self {
            compactor: Compactor::new(data_dir, config),
            stats_collector: StatsCollector::new(data_dir),
        }
    }

    #[doc = include_str!("../docs/MaintenanceApi.stats.md")]
    pub fn stats(&self) -> Result<DatabaseStats, String> {
        self.stats_collector.collect_database_stats()
    }

    #[doc = include_str!("../docs/MaintenanceApi.model_stats.md")]
    pub fn model_stats(&self, model_name: &str) -> Result<ModelStats, String> {
        self.stats_collector.collect_model_stats(model_name)
    }

    #[doc = include_str!("../docs/MaintenanceApi.compact_model.md")]
    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String> {
        self.compactor.compact_model(model_name)
    }

    #[doc = include_str!("../docs/MaintenanceApi.compact_all.md")]
    pub fn compact_all(&self) -> Result<Vec<CompactionResult>, String> {
        self.compactor.compact_all()
    }

    #[doc = include_str!("../docs/MaintenanceApi.compact_needed.md")]
    pub fn compact_needed(&self) -> Result<Vec<CompactionResult>, String> {
        self.compactor.compact_needed()
    }

    #[doc = include_str!("../docs/MaintenanceApi.vacuum.md")]
    pub fn vacuum(&self) -> Result<Vec<CompactionResult>, String> {
        self.compact_all()
    }

    #[doc = include_str!("../docs/MaintenanceApi.analyze.md")]
    pub fn analyze(&self) -> Result<DatabaseStats, String> {
        self.stats()
    }
}
