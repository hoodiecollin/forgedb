use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for compaction behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Minimum percentage of dead space to trigger compaction (0.0 - 1.0)
    pub dead_space_threshold: f64,

    /// Enable automatic background compaction
    pub auto_compact: bool,

    /// Interval between compaction checks (in seconds)
    pub check_interval_secs: u64,

    /// Maximum time a compaction can run (in seconds)
    pub max_compaction_time_secs: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            dead_space_threshold: 0.3, // 30% dead space triggers compaction
            auto_compact: true,
            check_interval_secs: 300, // Check every 5 minutes
            max_compaction_time_secs: 600, // Max 10 minutes per compaction
        }
    }
}

/// Statistics about a column's storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    /// Column name
    pub name: String,

    /// Column type (fixed or variable)
    pub column_type: ColumnType,

    /// Total bytes allocated
    pub total_bytes: u64,

    /// Bytes used (excluding dead space)
    pub used_bytes: u64,

    /// Bytes of dead space
    pub dead_bytes: u64,

    /// Number of active rows
    pub active_rows: usize,

    /// Number of deleted rows (tombstones)
    pub deleted_rows: usize,

    /// Percentage of dead space (0.0 - 1.0)
    pub dead_space_ratio: f64,
}

impl ColumnStats {
    /// Calculate dead space ratio
    pub fn calculate_ratio(&mut self) {
        if self.total_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_bytes as f64;
        }
    }
}

/// Type of column storage
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    Fixed,
    Variable,
}

/// Statistics about a model's storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    /// Model name
    pub name: String,

    /// Total rows (including deleted)
    pub total_rows: usize,

    /// Active rows (excluding deleted)
    pub active_rows: usize,

    /// Deleted rows
    pub deleted_rows: usize,

    /// Statistics per column
    pub columns: Vec<ColumnStats>,

    /// Total disk usage in bytes
    pub total_disk_bytes: u64,

    /// Total used bytes (excluding dead space)
    pub used_bytes: u64,

    /// Total dead bytes
    pub dead_bytes: u64,

    /// Overall dead space ratio
    pub dead_space_ratio: f64,

    /// Index sizes (index name -> size in bytes)
    pub index_sizes: HashMap<String, u64>,

    /// Last compaction time
    pub last_compaction: Option<DateTime<Utc>>,
}

impl ModelStats {
    /// Calculate overall dead space ratio
    pub fn calculate_ratio(&mut self) {
        if self.total_disk_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_disk_bytes as f64;
        }
    }

    /// Check if compaction is recommended based on config
    pub fn needs_compaction(&self, config: &CompactionConfig) -> bool {
        self.dead_space_ratio >= config.dead_space_threshold
    }
}

/// Database-wide statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// Statistics per model
    pub models: Vec<ModelStats>,

    /// Total disk usage in bytes
    pub total_disk_bytes: u64,

    /// Total used bytes
    pub used_bytes: u64,

    /// Total dead bytes
    pub dead_bytes: u64,

    /// Overall dead space ratio
    pub dead_space_ratio: f64,

    /// Statistics collection time
    pub collected_at: DateTime<Utc>,
}

impl DatabaseStats {
    /// Calculate overall dead space ratio
    pub fn calculate_ratio(&mut self) {
        if self.total_disk_bytes == 0 {
            self.dead_space_ratio = 0.0;
        } else {
            self.dead_space_ratio = self.dead_bytes as f64 / self.total_disk_bytes as f64;
        }
    }

    /// Get models that need compaction
    pub fn models_needing_compaction(&self, config: &CompactionConfig) -> Vec<&ModelStats> {
        self.models
            .iter()
            .filter(|m| m.needs_compaction(config))
            .collect()
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Model name
    pub model_name: String,

    /// Bytes before compaction
    pub bytes_before: u64,

    /// Bytes after compaction
    pub bytes_after: u64,

    /// Bytes reclaimed
    pub bytes_reclaimed: u64,

    /// Time taken (in milliseconds)
    pub duration_ms: u64,

    /// Compaction timestamp
    pub completed_at: DateTime<Utc>,

    /// Whether compaction succeeded
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,
}

impl CompactionResult {
    /// Calculate percentage of space reclaimed
    pub fn reclaim_percentage(&self) -> f64 {
        if self.bytes_before == 0 {
            0.0
        } else {
            (self.bytes_reclaimed as f64 / self.bytes_before as f64) * 100.0
        }
    }
}

/// Compaction operation status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionStatus {
    Idle,
    Running,
    Completed,
    Failed,
}
