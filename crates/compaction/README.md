# ForgeDB Compaction

Database compaction and space reclamation for ForgeDB's columnar storage system.

## Overview

The `forgedb-compaction` crate provides comprehensive database maintenance capabilities for ForgeDB, focusing on reclaiming dead space from deleted records, collecting storage statistics, and managing background compaction operations. It works with ForgeDB's columnar storage layout, handling both fixed-size and variable-length data.

## Features

- **Dead Space Reclamation** - Remove tombstoned rows from both fixed and variable-length columns
- **Statistics Collection** - Track storage usage, dead space ratios, and row counts per model and database-wide
- **Background Compaction** - Automated compaction runs on configurable intervals
- **Threshold-Based Triggering** - Smart compaction that only runs when dead space exceeds thresholds
- **Manual Compaction** - On-demand compaction for specific models or entire database
- **Performance Tracking** - Detailed metrics on bytes reclaimed, duration, and efficiency
- **Atomic Operations** - Safe file replacement ensures data integrity during compaction

## Usage Examples

### Basic Manual Compaction

```rust
use forgedb_compaction::{Compactor, CompactionConfig};

fn main() {
    let config = CompactionConfig::default();
    let compactor = Compactor::new("./data", config);
    
    // Compact a specific model
    match compactor.compact_model("User") {
        Ok(result) => {
            println!("Compacted {}: Reclaimed {} bytes ({:.1}%) in {}ms",
                result.model_name,
                result.bytes_reclaimed,
                result.reclaim_percentage(),
                result.duration_ms
            );
        }
        Err(e) => eprintln!("Compaction failed: {}", e),
    }
}
```

### Background Compaction

```rust
use forgedb_compaction::{BackgroundCompactor, CompactionConfig};

fn main() {
    let config = CompactionConfig {
        dead_space_threshold: 0.3,  // Compact when 30% dead space
        auto_compact: true,
        check_interval_secs: 300,    // Check every 5 minutes
        max_compaction_time_secs: 600,
    };
    
    let bg_compactor = BackgroundCompactor::new("./data", config);
    
    // Start background compaction
    bg_compactor.start();
    println!("Background compaction started");
    
    // Your application runs...
    
    // Check status
    println!("Status: {:?}", bg_compactor.status());
    
    // Get last results
    for result in bg_compactor.last_results() {
        if result.success {
            println!("{}: Reclaimed {} bytes", 
                result.model_name, 
                result.bytes_reclaimed
            );
        }
    }
    
    // Stop background compaction
    bg_compactor.stop();
}
```

### Statistics Collection

```rust
use forgedb_compaction::StatsCollector;

fn main() {
    let collector = StatsCollector::new("./data");
    
    // Collect database-wide statistics
    match collector.collect_database_stats() {
        Ok(stats) => {
            println!("Total disk usage: {} bytes", stats.total_disk_bytes);
            println!("Dead space: {} bytes ({:.1}%)", 
                stats.dead_bytes,
                stats.dead_space_ratio * 100.0
            );
            
            for model in stats.models {
                println!("\nModel: {}", model.name);
                println!("  Active rows: {}", model.active_rows);
                println!("  Deleted rows: {}", model.deleted_rows);
                println!("  Dead space ratio: {:.1}%", 
                    model.dead_space_ratio * 100.0
                );
                
                for column in model.columns {
                    println!("    Column {}: {} bytes ({:.1}% dead)",
                        column.name,
                        column.total_bytes,
                        column.dead_space_ratio * 100.0
                    );
                }
            }
        }
        Err(e) => eprintln!("Failed to collect stats: {}", e),
    }
}
```

### Selective Compaction (Only Models That Need It)

```rust
use forgedb_compaction::{Compactor, CompactionConfig};

fn main() {
    let config = CompactionConfig {
        dead_space_threshold: 0.25,  // 25% threshold
        ..Default::default()
    };
    
    let compactor = Compactor::new("./data", config);
    
    // Only compact models exceeding the threshold
    match compactor.compact_needed() {
        Ok(results) => {
            if results.is_empty() {
                println!("No models need compaction");
            } else {
                for result in results {
                    if result.success {
                        println!("{}: {:.1}% space reclaimed",
                            result.model_name,
                            result.reclaim_percentage()
                        );
                    }
                }
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### Using the Maintenance API

```rust
use forgedb_compaction::{MaintenanceApi, CompactionConfig};

fn main() {
    let config = CompactionConfig::default();
    let api = MaintenanceApi::new("./data", config);
    
    // Collect statistics
    let stats = api.analyze().unwrap();
    println!("Database has {:.1}% dead space", stats.dead_space_ratio * 100.0);
    
    // Run vacuum (compaction)
    let results = api.vacuum().unwrap();
    for result in results {
        println!("Vacuumed {}: {} bytes reclaimed",
            result.model_name,
            result.bytes_reclaimed
        );
    }
    
    // Get stats for specific model
    let user_stats = api.model_stats("User").unwrap();
    println!("User model: {} active rows, {} deleted",
        user_stats.active_rows,
        user_stats.deleted_rows
    );
}
```

## Compaction Strategies

### When Compaction Runs

Compaction removes deleted rows from the database storage, reclaiming disk space and improving read performance. There are several strategies for triggering compaction:

1. **Manual Compaction** - Explicitly called by your application
   - `compact_model(name)` - Compact a specific model
   - `compact_all()` - Compact all models
   - `compact_needed()` - Compact only models exceeding threshold

2. **Background Compaction** - Runs automatically on a schedule
   - Checks at configurable intervals (default: 5 minutes)
   - Only compacts models exceeding the threshold
   - Non-blocking, runs in background thread

3. **Threshold-Based** - Compaction triggered when dead space ratio exceeds limit
   - Default threshold: 30% dead space
   - Prevents unnecessary compaction of mostly-active data
   - Balances I/O cost with space reclamation benefit

### Threshold Configuration

The `dead_space_threshold` determines when a model needs compaction:

```rust
// Aggressive compaction (compact at 10% dead space)
let config = CompactionConfig {
    dead_space_threshold: 0.1,
    ..Default::default()
};

// Conservative compaction (compact at 50% dead space)
let config = CompactionConfig {
    dead_space_threshold: 0.5,
    ..Default::default()
};
```

**Recommendations:**
- **High-write workloads**: 0.3-0.5 (30-50%) - Less frequent compaction
- **High-read workloads**: 0.1-0.2 (10-20%) - More frequent compaction for better read performance
- **Balanced workloads**: 0.3 (30%) - Default, good balance

### Compaction Process

1. **Read Current State** - Collect statistics and read tombstone bitmap
2. **Compact Variable Columns** - Rebuild data files with only active rows
3. **Compact Fixed Columns** - Remove deleted row slots from fixed-size columns
4. **Update Tombstones** - Reset tombstone bitmap (no deleted rows after compaction)
5. **Atomic Replacement** - Safely replace old files with compacted versions
6. **Record Timestamp** - Mark last compaction time for tracking

### Performance Impact

Compaction is an I/O-intensive operation with these characteristics:

- **Disk I/O**: Reads and rewrites all column files for compacted models
- **CPU**: Minimal - mostly copying data, no complex processing
- **Memory**: Buffers column data during compaction
- **Concurrency**: Safe to run during normal database operations (uses atomic file replacement)

**Best Practices:**
- Run background compaction during low-traffic periods if possible
- Set reasonable thresholds to avoid over-compaction
- Monitor compaction metrics to tune `check_interval_secs`
- Use `compact_needed()` instead of `compact_all()` to minimize unnecessary work

## API Reference

### Compactor

The core compaction engine that performs space reclamation.

```rust
pub struct Compactor {
    // ...
}

impl Compactor {
    /// Create a new compactor
    pub fn new<P: AsRef<Path>>(data_dir: P, config: CompactionConfig) -> Self
    
    /// Compact a specific model
    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String>
    
    /// Compact all models in the database
    pub fn compact_all(&self) -> Result<Vec<CompactionResult>, String>
    
    /// Compact only models exceeding the dead space threshold
    pub fn compact_needed(&self) -> Result<Vec<CompactionResult>, String>
}
```

### BackgroundCompactor

Manages automated compaction in a background thread.

```rust
pub struct BackgroundCompactor {
    // ...
}

impl BackgroundCompactor {
    /// Create a new background compactor
    pub fn new<P: AsRef<Path>>(data_dir: P, config: CompactionConfig) -> Self
    
    /// Start the background compaction thread
    pub fn start(&self)
    
    /// Stop the background compaction thread
    pub fn stop(&self)
    
    /// Check if background compaction is running
    pub fn is_running(&self) -> bool
    
    /// Get current compaction status
    pub fn status(&self) -> CompactionStatus
    
    /// Get results from the last compaction run
    pub fn last_results(&self) -> Vec<CompactionResult>
    
    /// Trigger a manual compaction (non-blocking)
    pub fn trigger_manual(&self) -> Result<(), String>
}
```

### StatsCollector

Collects storage statistics for analysis and monitoring.

```rust
pub struct StatsCollector {
    // ...
}

impl StatsCollector {
    /// Create a new statistics collector
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self
    
    /// Collect statistics for a specific model
    pub fn collect_model_stats(&self, model_name: &str) -> Result<ModelStats, String>
    
    /// Collect statistics for the entire database
    pub fn collect_database_stats(&self) -> Result<DatabaseStats, String>
}
```

### MaintenanceApi

High-level API combining compaction and statistics.

```rust
pub struct MaintenanceApi {
    // ...
}

impl MaintenanceApi {
    /// Create a new maintenance API instance
    pub fn new<P: AsRef<Path>>(data_dir: P, config: CompactionConfig) -> Self
    
    /// Collect database statistics
    pub fn stats(&self) -> Result<DatabaseStats, String>
    
    /// Collect statistics for a specific model
    pub fn model_stats(&self, model_name: &str) -> Result<ModelStats, String>
    
    /// Manually trigger compaction for a specific model
    pub fn compact_model(&self, model_name: &str) -> Result<CompactionResult, String>
    
    /// Compact all models
    pub fn compact_all(&self) -> Result<Vec<CompactionResult>, String>
    
    /// Compact only models that need it (based on threshold)
    pub fn compact_needed(&self) -> Result<Vec<CompactionResult>, String>
    
    /// Vacuum operation - alias for compact_all (removes dead space)
    pub fn vacuum(&self) -> Result<Vec<CompactionResult>, String>
    
    /// Analyze operation - collect and return statistics
    pub fn analyze(&self) -> Result<DatabaseStats, String>
}
```

## Configuration

### CompactionConfig

```rust
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
```

**Default values:**
```rust
CompactionConfig {
    dead_space_threshold: 0.3,          // 30%
    auto_compact: true,
    check_interval_secs: 300,           // 5 minutes
    max_compaction_time_secs: 600,      // 10 minutes
}
```

### Configuration Examples

**Development (aggressive compaction for testing):**
```rust
CompactionConfig {
    dead_space_threshold: 0.1,      // Compact at 10% dead
    auto_compact: true,
    check_interval_secs: 60,         // Check every minute
    max_compaction_time_secs: 300,
}
```

**Production (balanced):**
```rust
CompactionConfig {
    dead_space_threshold: 0.3,       // Compact at 30% dead
    auto_compact: true,
    check_interval_secs: 600,        // Check every 10 minutes
    max_compaction_time_secs: 1800,  // Allow up to 30 minutes
}
```

**Manual-only (no background compaction):**
```rust
CompactionConfig {
    dead_space_threshold: 0.4,
    auto_compact: false,             // Disable background compaction
    check_interval_secs: 3600,
    max_compaction_time_secs: 600,
}
```

## Performance

### Space Reclamation Metrics

Compaction provides detailed metrics for each operation:

```rust
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
    /// Calculate percentage of space reclaimed
    pub fn reclaim_percentage(&self) -> f64
}
```

**Example metrics:**
```
Model: User
  Before: 1,000,000 bytes
  After: 700,000 bytes
  Reclaimed: 300,000 bytes (30%)
  Duration: 45ms
```

### Impact on Database Operations

**During Compaction:**
- **Reads**: Continue normally from original files
- **Writes**: Continue normally, new data appended
- **Deletes**: Continue normally, tombstones marked
- **File Replacement**: Atomic, no lock-up period

**After Compaction:**
- **Read Performance**: Improved due to reduced file sizes
- **Disk Space**: Freed up by removed dead space
- **Tombstone Bitmap**: Reset to all-active state
- **Index Performance**: Unchanged (indexes not stored on disk currently)

### Performance Characteristics

- **Fixed Columns**: O(n) where n = number of rows, fast sequential I/O
- **Variable Columns**: O(n) with offset recalculation, still sequential I/O
- **Tombstone Compaction**: O(n) bitmap processing
- **File Operations**: Atomic rename, no data corruption risk

**Typical Performance (1M rows, 30% deleted):**
- Fixed-size column (UUID): ~50-100ms
- Variable-length column (email): ~100-200ms  
- Full model compaction: ~200-500ms
- Database-wide compaction (10 models): ~2-5s

## Testing

### Running Tests

```bash
# Run all tests
cargo test -p forgedb-compaction

# Run with output
cargo test -p forgedb-compaction -- --nocapture

# Run specific test
cargo test -p forgedb-compaction test_compact_model
```

### Test Coverage

The crate includes comprehensive tests for:

- **Compactor Tests** (`compactor_tests.rs`)
  - Variable column compaction
  - Fixed column compaction
  - Model-level compaction
  - Space reclamation verification

- **Background Tests** (`background_tests.rs`)
  - Background thread lifecycle
  - Manual trigger functionality
  - Status tracking

- **Statistics Tests** (`stats_tests.rs`)
  - Model statistics collection
  - Database-wide statistics
  - Dead space ratio calculation

- **Integration Tests** (`lib_tests.rs`)
  - Maintenance API operations
  - Vacuum and analyze operations

### Example Test

```rust
#[test]
fn test_compact_reclaims_space() {
    let temp_dir = TempDir::new().unwrap();
    let model_dir = temp_dir.path().join("User");
    
    // Create model with 50% dead space
    setup_model_with_deletions(&model_dir, 0.5);
    
    let config = CompactionConfig::default();
    let compactor = Compactor::new(temp_dir.path(), config);
    
    let result = compactor.compact_model("User").unwrap();
    
    assert!(result.success);
    assert!(result.bytes_reclaimed > 0);
    assert!(result.reclaim_percentage() > 40.0);
}
```

## Integration with ForgeDB

This crate is part of the ForgeDB ecosystem and works with:

- **forgedb-storage**: Reads/writes columnar storage files
- **forgedb-types**: Uses common data structures
- **forgedb**: Main database integration point

## Documentation

For more information about ForgeDB:

- **[ForgeDB Architecture](../../docs/ARCHITECTURE.md)** - System design and component architecture
- **[Public Crates Guide](../../docs/PUBLIC_CRATES.md)** - Complete runtime library documentation
- **[Development Guide](../../docs/DEVELOPMENT.md)** - Development setup and workflow
- **[Contributing Guide](../../docs/CONTRIBUTING.md)** - How to contribute to ForgeDB

## License

Part of the ForgeDB project.
