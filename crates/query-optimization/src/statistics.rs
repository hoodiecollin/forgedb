// Index statistics tracking for query optimization
//
// This module tracks:
// - Index usage counts
// - Index selectivity
// - Cardinality estimates
// - Cache hit rates

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Statistics for a single index
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
    pub is_unique: bool,

    /// Total number of lookups performed
    pub lookup_count: u64,
    /// Total number of range scans performed
    pub range_scan_count: u64,
    /// Number of rows in the index
    pub row_count: usize,
    /// Number of distinct values (cardinality)
    pub cardinality: usize,
    /// Size of index in bytes
    pub size_bytes: usize,

    /// Last time statistics were updated
    pub last_updated: SystemTime,

    /// Histogram of selectivity values (for query planning)
    pub selectivity_histogram: Vec<f64>,
}

impl IndexStats {
    pub fn new(
        name: String,
        table: String,
        columns: Vec<String>,
        is_unique: bool,
    ) -> Self {
        IndexStats {
            name,
            table,
            columns,
            is_unique,
            lookup_count: 0,
            range_scan_count: 0,
            row_count: 0,
            cardinality: 0,
            size_bytes: 0,
            last_updated: SystemTime::now(),
            selectivity_histogram: Vec::new(),
        }
    }

    /// Record an index lookup
    pub fn record_lookup(&mut self) {
        self.lookup_count += 1;
        self.last_updated = SystemTime::now();
    }

    /// Record a range scan
    pub fn record_range_scan(&mut self) {
        self.range_scan_count += 1;
        self.last_updated = SystemTime::now();
    }

    /// Update cardinality estimate
    pub fn update_cardinality(&mut self, cardinality: usize) {
        self.cardinality = cardinality;
        self.last_updated = SystemTime::now();
    }

    /// Update row count
    pub fn update_row_count(&mut self, row_count: usize) {
        self.row_count = row_count;
        self.last_updated = SystemTime::now();
    }

    /// Update index size
    pub fn update_size(&mut self, size_bytes: usize) {
        self.size_bytes = size_bytes;
        self.last_updated = SystemTime::now();
    }

    /// Calculate average selectivity
    pub fn avg_selectivity(&self) -> f64 {
        if self.row_count == 0 {
            return 1.0;
        }

        self.cardinality as f64 / self.row_count as f64
    }

    /// Get total operations (lookups + scans)
    pub fn total_operations(&self) -> u64 {
        self.lookup_count + self.range_scan_count
    }

    /// Check if statistics are stale (older than threshold)
    pub fn is_stale(&self, threshold: Duration) -> bool {
        SystemTime::now()
            .duration_since(self.last_updated)
            .unwrap_or(Duration::from_secs(0))
            > threshold
    }
}

/// Statistics collector for all indexes
pub struct IndexStatistics {
    stats: HashMap<String, IndexStats>,
    stale_threshold: Duration,
}

impl IndexStatistics {
    pub fn new() -> Self {
        IndexStatistics {
            stats: HashMap::new(),
            stale_threshold: Duration::from_secs(3600), // 1 hour default
        }
    }

    /// Set the staleness threshold
    pub fn set_stale_threshold(&mut self, threshold: Duration) {
        self.stale_threshold = threshold;
    }

    /// Register a new index
    pub fn register_index(
        &mut self,
        name: String,
        table: String,
        columns: Vec<String>,
        is_unique: bool,
    ) {
        let stats = IndexStats::new(name.clone(), table, columns, is_unique);
        self.stats.insert(name, stats);
    }

    /// Get statistics for an index
    pub fn get_stats(&self, index_name: &str) -> Option<&IndexStats> {
        self.stats.get(index_name)
    }

    /// Get mutable statistics for an index
    pub fn get_stats_mut(&mut self, index_name: &str) -> Option<&mut IndexStats> {
        self.stats.get_mut(index_name)
    }

    /// Record an index lookup
    pub fn record_lookup(&mut self, index_name: &str) {
        if let Some(stats) = self.stats.get_mut(index_name) {
            stats.record_lookup();
        }
    }

    /// Record a range scan
    pub fn record_range_scan(&mut self, index_name: &str) {
        if let Some(stats) = self.stats.get_mut(index_name) {
            stats.record_range_scan();
        }
    }

    /// Update index statistics
    pub fn update_index_stats(
        &mut self,
        index_name: &str,
        row_count: usize,
        cardinality: usize,
        size_bytes: usize,
    ) {
        if let Some(stats) = self.stats.get_mut(index_name) {
            stats.update_row_count(row_count);
            stats.update_cardinality(cardinality);
            stats.update_size(size_bytes);
        }
    }

    /// Get all indexes that need statistics refresh
    pub fn get_stale_indexes(&self) -> Vec<String> {
        self.stats
            .iter()
            .filter(|(_, stats)| stats.is_stale(self.stale_threshold))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get indexes sorted by usage (most used first)
    pub fn get_most_used_indexes(&self, limit: usize) -> Vec<(String, u64)> {
        let mut usage: Vec<_> = self
            .stats
            .iter()
            .map(|(name, stats)| (name.clone(), stats.total_operations()))
            .collect();

        usage.sort_by(|a, b| b.1.cmp(&a.1));
        usage.into_iter().take(limit).collect()
    }

    /// Get total size of all indexes
    pub fn total_index_size(&self) -> usize {
        self.stats.values().map(|s| s.size_bytes).sum()
    }

    /// Get all statistics
    pub fn all_stats(&self) -> Vec<&IndexStats> {
        self.stats.values().collect()
    }

    /// Clear statistics for an index
    pub fn clear_stats(&mut self, index_name: &str) {
        if let Some(stats) = self.stats.get_mut(index_name) {
            stats.lookup_count = 0;
            stats.range_scan_count = 0;
            stats.last_updated = SystemTime::now();
        }
    }

    /// Remove an index from tracking
    pub fn remove_index(&mut self, index_name: &str) -> Option<IndexStats> {
        self.stats.remove(index_name)
    }
}

impl Default for IndexStatistics {
    fn default() -> Self {
        Self::new()
    }
}

