use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[doc = include_str!("../docs/types.CompactionConfig.md")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    #[doc = include_str!("../docs/types.CompactionConfig.dead_space_threshold.md")]
    pub dead_space_threshold: f64,

    #[doc = include_str!("../docs/types.CompactionConfig.auto_compact.md")]
    pub auto_compact: bool,

    #[doc = include_str!("../docs/types.CompactionConfig.check_interval_secs.md")]
    pub check_interval_secs: u64,

    #[doc = include_str!("../docs/types.CompactionConfig.max_compaction_time_secs.md")]
    pub max_compaction_time_secs: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            dead_space_threshold: 0.3,
            auto_compact: true,
            check_interval_secs: 300,
            max_compaction_time_secs: 600,
        }
    }
}

#[doc = include_str!("../docs/types.ColumnStats.md")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    #[doc = include_str!("../docs/types.ColumnStats.name.md")]
    pub name: String,

    #[doc = include_str!("../docs/types.ColumnStats.column_type.md")]
    pub column_type: ColumnType,

    #[doc = include_str!("../docs/types.ColumnStats.total_bytes.md")]
    pub total_bytes: u64,

    #[doc = include_str!("../docs/types.ColumnStats.used_bytes.md")]
    pub used_bytes: u64,

    #[doc = include_str!("../docs/types.ColumnStats.dead_bytes.md")]
    pub dead_bytes: u64,

    #[doc = include_str!("../docs/types.ColumnStats.active_rows.md")]
    pub active_rows: usize,

    #[doc = include_str!("../docs/types.ColumnStats.deleted_rows.md")]
    pub deleted_rows: usize,

    #[doc = include_str!("../docs/types.ColumnStats.dead_space_ratio.md")]
    pub dead_space_ratio: f64,
}

impl ColumnStats {
    #[doc = include_str!("../docs/types.ColumnStats.calculate_ratio.md")]
    pub fn calculate_ratio(&mut self) {
        if self.total_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_bytes as f64;
        }
    }
}

#[doc = include_str!("../docs/types.ColumnType.md")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    #[doc = include_str!("../docs/types.ColumnType.Fixed.md")]
    Fixed,
    #[doc = include_str!("../docs/types.ColumnType.Variable.md")]
    Variable,
}

#[doc = include_str!("../docs/types.ModelStats.md")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    #[doc = include_str!("../docs/types.ModelStats.name.md")]
    pub name: String,

    #[doc = include_str!("../docs/types.ModelStats.total_rows.md")]
    pub total_rows: usize,

    #[doc = include_str!("../docs/types.ModelStats.active_rows.md")]
    pub active_rows: usize,

    #[doc = include_str!("../docs/types.ModelStats.deleted_rows.md")]
    pub deleted_rows: usize,

    #[doc = include_str!("../docs/types.ModelStats.columns.md")]
    pub columns: Vec<ColumnStats>,

    #[doc = include_str!("../docs/types.ModelStats.total_disk_bytes.md")]
    pub total_disk_bytes: u64,

    #[doc = include_str!("../docs/types.ModelStats.used_bytes.md")]
    pub used_bytes: u64,

    #[doc = include_str!("../docs/types.ModelStats.dead_bytes.md")]
    pub dead_bytes: u64,

    #[doc = include_str!("../docs/types.ModelStats.dead_space_ratio.md")]
    pub dead_space_ratio: f64,

    #[doc = include_str!("../docs/types.ModelStats.index_sizes.md")]
    pub index_sizes: HashMap<String, u64>,

    #[doc = include_str!("../docs/types.ModelStats.last_compaction.md")]
    pub last_compaction: Option<DateTime<Utc>>,
}

impl ModelStats {
    #[doc = include_str!("../docs/types.ModelStats.calculate_ratio.md")]
    pub fn calculate_ratio(&mut self) {
        if self.total_disk_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_disk_bytes as f64;
        }
    }

    #[doc = include_str!("../docs/types.ModelStats.needs_compaction.md")]
    pub fn needs_compaction(&self, config: &CompactionConfig) -> bool {
        self.dead_space_ratio >= config.dead_space_threshold
    }
}

#[doc = include_str!("../docs/types.DatabaseStats.md")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    #[doc = include_str!("../docs/types.DatabaseStats.models.md")]
    pub models: Vec<ModelStats>,

    #[doc = include_str!("../docs/types.DatabaseStats.total_disk_bytes.md")]
    pub total_disk_bytes: u64,

    #[doc = include_str!("../docs/types.DatabaseStats.used_bytes.md")]
    pub used_bytes: u64,

    #[doc = include_str!("../docs/types.DatabaseStats.dead_bytes.md")]
    pub dead_bytes: u64,

    #[doc = include_str!("../docs/types.DatabaseStats.dead_space_ratio.md")]
    pub dead_space_ratio: f64,

    #[doc = include_str!("../docs/types.DatabaseStats.collected_at.md")]
    pub collected_at: DateTime<Utc>,
}

impl DatabaseStats {
    #[doc = include_str!("../docs/types.DatabaseStats.calculate_ratio.md")]
    pub fn calculate_ratio(&mut self) {
        if self.total_disk_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_disk_bytes as f64;
        }
    }

    #[doc = include_str!("../docs/types.DatabaseStats.models_needing_compaction.md")]
    pub fn models_needing_compaction(&self, config: &CompactionConfig) -> Vec<&ModelStats> {
        self.models
            .iter()
            .filter(|m| m.needs_compaction(config))
            .collect()
    }
}

#[doc = include_str!("../docs/types.CompactionResult.md")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    #[doc = include_str!("../docs/types.CompactionResult.model_name.md")]
    pub model_name: String,

    #[doc = include_str!("../docs/types.CompactionResult.bytes_before.md")]
    pub bytes_before: u64,

    #[doc = include_str!("../docs/types.CompactionResult.bytes_after.md")]
    pub bytes_after: u64,

    #[doc = include_str!("../docs/types.CompactionResult.bytes_reclaimed.md")]
    pub bytes_reclaimed: u64,

    #[doc = include_str!("../docs/types.CompactionResult.duration_ms.md")]
    pub duration_ms: u64,

    #[doc = include_str!("../docs/types.CompactionResult.completed_at.md")]
    pub completed_at: DateTime<Utc>,

    #[doc = include_str!("../docs/types.CompactionResult.success.md")]
    pub success: bool,

    #[doc = include_str!("../docs/types.CompactionResult.error.md")]
    pub error: Option<String>,
}

impl CompactionResult {
    #[doc = include_str!("../docs/types.CompactionResult.reclaim_percentage.md")]
    pub fn reclaim_percentage(&self) -> f64 {
        if self.bytes_before == 0 {
            0.0
        } else {
            (self.bytes_reclaimed as f64 / self.bytes_before as f64) * 100.0
        }
    }
}

#[doc = include_str!("../docs/types.CompactionStatus.md")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionStatus {
    #[doc = include_str!("../docs/types.CompactionStatus.Idle.md")]
    Idle,
    #[doc = include_str!("../docs/types.CompactionStatus.Running.md")]
    Running,
    #[doc = include_str!("../docs/types.CompactionStatus.Completed.md")]
    Completed,
    #[doc = include_str!("../docs/types.CompactionStatus.Failed.md")]
    Failed,
}
