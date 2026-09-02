use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub dead_space_threshold: f64,

    pub auto_compact: bool,

    pub check_interval_secs: u64,

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub name: String,

    pub column_type: ColumnType,

    pub total_bytes: u64,

    pub used_bytes: u64,

    pub dead_bytes: u64,

    pub active_rows: usize,

    pub deleted_rows: usize,

    pub dead_space_ratio: f64,
}

impl ColumnStats {
    pub fn calculate_ratio(&mut self) {
        if self.total_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_bytes as f64;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    Fixed,
    Variable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub name: String,

    pub total_rows: usize,

    pub active_rows: usize,

    pub deleted_rows: usize,

    pub columns: Vec<ColumnStats>,

    pub total_disk_bytes: u64,

    pub used_bytes: u64,

    pub dead_bytes: u64,

    pub dead_space_ratio: f64,

    pub index_sizes: HashMap<String, u64>,

    pub last_compaction: Option<DateTime<Utc>>,
}

impl ModelStats {
    pub fn calculate_ratio(&mut self) {
        if self.total_disk_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_disk_bytes as f64;
        }
    }

    pub fn needs_compaction(&self, config: &CompactionConfig) -> bool {
        self.dead_space_ratio >= config.dead_space_threshold
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub models: Vec<ModelStats>,

    pub total_disk_bytes: u64,

    pub used_bytes: u64,

    pub dead_bytes: u64,

    pub dead_space_ratio: f64,

    pub collected_at: DateTime<Utc>,
}

impl DatabaseStats {
    pub fn calculate_ratio(&mut self) {
        if self.total_disk_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_disk_bytes as f64;
        }
    }

    pub fn models_needing_compaction(&self, config: &CompactionConfig) -> Vec<&ModelStats> {
        self.models
            .iter()
            .filter(|m| m.needs_compaction(config))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub model_name: String,

    pub bytes_before: u64,

    pub bytes_after: u64,

    pub bytes_reclaimed: u64,

    pub duration_ms: u64,

    pub completed_at: DateTime<Utc>,

    pub success: bool,

    pub error: Option<String>,
}

impl CompactionResult {
    pub fn reclaim_percentage(&self) -> f64 {
        if self.bytes_before == 0 {
            0.0
        } else {
            (self.bytes_reclaimed as f64 / self.bytes_before as f64) * 100.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionStatus {
    Idle,
    Running,
    Completed,
    Failed,
}
