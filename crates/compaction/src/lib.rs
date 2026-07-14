//! ForgeDB Compaction
//!
//! Database compaction and space reclamation for ForgeDB's columnar storage system.
//!
//! # Overview
//!
//! This crate provides comprehensive database maintenance capabilities for ForgeDB,
//! focusing on reclaiming dead space from deleted records, collecting storage statistics,
//! and managing background compaction operations.
//!
//! # Architecture
//!
//! The compaction system consists of three main components:
//!
//! - **Compactor**: Core compaction engine that performs space reclamation
//! - **BackgroundCompactor**: Manages automated compaction in a background thread
//! - **StatsCollector**: Collects storage statistics for analysis and monitoring
//!
//! ## Compaction Process
//!
//! 1. **Read Current State** - Collect statistics and read tombstone bitmap
//! 2. **Compact Variable Columns** - Rebuild data files with only active rows
//! 3. **Compact Fixed Columns** - Remove deleted row slots from fixed-size columns
//! 4. **Update Tombstones** - Reset tombstone bitmap (no deleted rows after compaction)
//! 5. **Atomic Replacement** - Safely replace old files with compacted versions
//!
//! # Examples
//!
//! ## Manual Compaction
//!
//! ```rust,no_run
//! use forgedb_compaction::{Compactor, CompactionConfig};
//!
//! let config = CompactionConfig::default();
//! let compactor = Compactor::new("./data", config);
//!
//! // Compact a specific model
//! match compactor.compact_model("User") {
//!     Ok(result) => {
//!         println!("Reclaimed {} bytes ({:.1}%)",
//!             result.bytes_reclaimed,
//!             result.reclaim_percentage()
//!         );
//!     }
//!     Err(e) => eprintln!("Compaction failed: {}", e),
//! }
//! ```
//!
//! ## Background Compaction
//!
//! ```rust,no_run
//! use forgedb_compaction::{BackgroundCompactor, CompactionConfig};
//!
//! let config = CompactionConfig {
//!     dead_space_threshold: 0.3,  // Compact when 30% dead space
//!     auto_compact: true,
//!     check_interval_secs: 300,    // Check every 5 minutes
//!     max_compaction_time_secs: 600,
//! };
//!
//! let bg_compactor = BackgroundCompactor::new("./data", config);
//! bg_compactor.start();
//!
//! // Your application runs...
//!
//! bg_compactor.stop();
//! ```
//!
//! ## Statistics Collection
//!
//! ```rust,no_run
//! use forgedb_compaction::StatsCollector;
//!
//! let collector = StatsCollector::new("./data");
//!
//! // Collect database-wide statistics
//! match collector.collect_database_stats() {
//!     Ok(stats) => {
//!         println!("Total disk usage: {} bytes", stats.total_disk_bytes);
//!         println!("Dead space: {:.1}%", stats.dead_space_ratio * 100.0);
//!     }
//!     Err(e) => eprintln!("Failed to collect stats: {}", e),
//! }
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`Compactor`] - Core compaction engine
//! - [`BackgroundCompactor`] - Automated background compaction
//! - [`StatsCollector`] - Storage statistics collector
//! - [`MaintenanceApi`] - High-level API combining compaction and statistics
//!
//! ## Configuration
//!
//! - [`CompactionConfig`] - Configuration for compaction behavior
//! - [`CompactionResult`] - Results and metrics from compaction operations
//! - [`DatabaseStats`] - Database-wide storage statistics
//! - [`ModelStats`] - Per-model storage statistics
//!
//! # Compaction Strategies
//!
//! ## Threshold-Based Compaction
//!
//! Compaction is triggered when dead space ratio exceeds a threshold:
//!
//! - **High-write workloads**: 0.3-0.5 (30-50%) - Less frequent compaction
//! - **High-read workloads**: 0.1-0.2 (10-20%) - More frequent compaction
//! - **Balanced workloads**: 0.3 (30%) - Default, good balance
//!
//! ## Performance Impact
//!
//! - **Disk I/O**: Reads and rewrites all column files for compacted models
//! - **CPU**: Minimal - mostly copying data
//! - **Memory**: Buffers column data during compaction
//! - **Concurrency**: Safe to run during normal database operations
//!
//! # Related Crates
//!
//! - [`forgedb-storage`](../forgedb_storage) - Columnar storage engine
//! - [`forgedb-types`](../forgedb_types) - Common type definitions
//!
//! # See Also
//!
//! - [README](./README.md) for comprehensive documentation and examples

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

    /// DEPRECATED — unsafe against the #66 mutation surface (see #105).
    ///
    /// Manually trigger the tombstone-based offline compaction for a specific
    /// model.  This reclaims nothing from superseding-version updates and
    /// RESURRECTS deleted rows.  Prefer the keep-set primitive
    /// [`Compactor::compact_model_keeping`], which the generated in-process
    /// `Database::compact()` (#92) drives.  Retained (not `#[deprecated]`) only
    /// to avoid a breaking change to the published `forgedb-compaction 0.1.0`
    /// API; the offline `forgedb compact` CLI no longer calls it.
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

