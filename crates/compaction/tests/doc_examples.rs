#![allow(deprecated)]

use forgedb_compaction::{BackgroundCompactor, CompactionConfig, Compactor, StatsCollector};

#[allow(dead_code)]
fn compact_a_single_model_compiles() {
    let config = CompactionConfig::default();
    let compactor = Compactor::new("./data", config);

    match compactor.compact_model("User") {
        Ok(result) => {
            let _ = (result.bytes_reclaimed, result.reclaim_percentage());
        }
        Err(e) => eprintln!("Compaction failed: {e}"),
    }
}

#[allow(dead_code)]
fn background_compactor_compiles() {
    let config = CompactionConfig {
        dead_space_threshold: 0.3,
        auto_compact: true,
        check_interval_secs: 300,
        max_compaction_time_secs: 600,
    };

    let bg_compactor = BackgroundCompactor::new("./data", config);
    bg_compactor.start();
    bg_compactor.stop();
}

#[allow(dead_code)]
fn stats_collector_compiles() {
    let collector = StatsCollector::new("./data");

    match collector.collect_database_stats() {
        Ok(stats) => {
            let _ = (stats.total_disk_bytes, stats.dead_space_ratio);
        }
        Err(e) => eprintln!("Failed to collect stats: {e}"),
    }
}
