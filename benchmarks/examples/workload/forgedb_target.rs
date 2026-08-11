//! ForgeDB [`WorkloadTarget`], over the 22-column `Metric` model.
//!
//! `Metric` is deliberately the subject: every column is fixed-width, so a scan here
//! exercises `FixedColumn::export` and nothing else. Running the same workload against
//! a model carrying a `String` would additionally drag `VariableColumn::gather_buffered`
//! through it, and the two costs would arrive summed and inseparable.
//!
//! Writes go through the storage-level `insert`/`update`/`delete` rather than the
//! `Database::create_metric` wrappers: `Metric` has no relations, so the wrappers add
//! only FK/cascade checks that would be pure no-op overhead here, and the point is to
//! measure the storage path.

use forgedb_benchmarks::forgedb_generated::{Database, Metric};
use forgedb_benchmarks::ts_from_seconds;
use uuid::Uuid;

use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};

/// Stable uuid for a workload key. Kind tag 4 keeps these from colliding with the
/// shared corpus's user/post/tag ids (1/2/3).
pub fn metric_id(key: u64) -> Uuid {
    Uuid::from_u128((4u128 << 96) | key as u128)
}

pub struct ForgeTarget {
    /// `Option` purely so `reopen` can drop the live handle before reopening: the data
    /// dir is under an exclusive `DirLock` (#89 single-writer-per-process), so opening
    /// a second handle without dropping the first would panic rather than measure.
    db: Option<Database>,
    dir: tempfile::TempDir,
    /// Bumped on every write so each update produces genuinely different bytes.
    /// Without this, an engine could in principle detect an unchanged row and skip
    /// the write, and the churn workload would quietly stop churning.
    generation: u64,
}

impl ForgeTarget {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Some(Database::open_at(dir.path().to_path_buf()));
        Self { db, dir, generation: 0 }
    }

    fn db(&self) -> &Database {
        self.db.as_ref().expect("database handle is only absent inside reopen")
    }

    fn db_mut(&mut self) -> &mut Database {
        self.db.as_mut().expect("database handle is only absent inside reopen")
    }

    fn row(&self, key: u64, g: u64) -> Metric {
        // Every field is a pure function of (key, generation): reproducible across
        // runs and identical to what the SQLite/redb targets store.
        let m = key.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ g.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        Metric {
            id: metric_id(key),
            recorded_at: ts_from_seconds(1_700_000_000 + (key as i64 % 100_000)),
            device_id: key % 1024,
            sample_seq: g,
            region: (m % 16) as u32,
            cpu_pct: (m % 10_000) as f64 / 100.0,
            mem_pct: ((m >> 8) % 10_000) as f64 / 100.0,
            disk_pct: ((m >> 16) % 10_000) as f64 / 100.0,
            net_rx_bytes: m % 1_000_000,
            net_tx_bytes: (m >> 4) % 1_000_000,
            req_count: (m >> 8) % 100_000,
            err_count: (m >> 12) % 1_000,
            p50_micros: ((m >> 16) % 10_000) as u32,
            p95_micros: ((m >> 20) % 50_000) as u32,
            p99_micros: ((m >> 24) % 100_000) as u32,
            queue_depth: ((m >> 28) % 512) as u32,
            open_conns: ((m >> 32) % 4096) as u32,
            gc_pause_micros: ((m >> 36) % 20_000) as u32,
            uptime_secs: (m % 10_000_000) as i64,
            temp_celsius: 20.0 + ((m >> 40) % 6000) as f64 / 100.0,
            throttled: m & 1 == 1,
            healthy: m & 2 == 0,
        }
    }
}

impl WorkloadTarget for ForgeTarget {
    fn name(&self) -> &'static str {
        "forgedb"
    }

    fn create(&mut self, key: u64) -> OpOutcome {
        self.generation += 1;
        let rec = self.row(key, self.generation);
        match self.db_mut().metric.insert(rec) {
            Ok(_) => OpOutcome::ok(),
            Err(_) => OpOutcome::miss(),
        }
    }

    fn read(&mut self, key: u64) -> OpOutcome {
        match self.db().metric.get(metric_id(key)) {
            Some(_) => OpOutcome::ok(),
            None => OpOutcome::miss(),
        }
    }

    fn update(&mut self, key: u64, width: UpdateWidth) -> OpOutcome {
        self.generation += 1;
        let id = metric_id(key);
        // `update` requires the full record either way — ForgeDB writes the whole row
        // regardless of what changed. OneField vs AllFields therefore differ in the
        // *bytes that actually differ*, not in what gets written; that asymmetry IS
        // the finding this axis exists to quantify.
        let rec = match width {
            UpdateWidth::AllFields => self.row(key, self.generation),
            UpdateWidth::OneField => match self.db().metric.get(id) {
                Some(mut cur) => {
                    cur.sample_seq = self.generation;
                    cur
                }
                None => return OpOutcome::miss(),
            },
        };
        match self.db_mut().metric.update(id, rec) {
            Ok(true) => OpOutcome::ok(),
            _ => OpOutcome::miss(),
        }
    }

    fn delete(&mut self, key: u64) -> OpOutcome {
        if self.db_mut().metric.delete(metric_id(key)) {
            OpOutcome::ok()
        } else {
            OpOutcome::miss()
        }
    }

    fn scan(&mut self, kind: ScanKind, limit: usize) -> OpOutcome {
        let n = match kind {
            // Declared `@projection(hot: cpu_pct, mem_pct)` → column-pruned buffered
            // scan → `FixedColumn::export`.
            ScanKind::Projection => self.db().metric.all_hot().len(),
            // Internal narrow scan: id + every filterable/sortable column. A SCOPE
            // since #228 (`__with_scan`) — the per-row refs are still built eagerly
            // from the bulk-loaded column buffers, so this is the same decode the
            // owned `__scan_all()` used to measure, minus the row materialization.
            ScanKind::Narrow => self
                .db()
                .metric
                .__with_scan(None, |_| true, |scan| scan.len()),
        };
        OpOutcome::rows(n.min(limit) as u64)
    }

    fn maintain(&mut self) {
        self.db_mut().compact();
        self.db_mut().checkpoint();
    }

    fn footprint(&self) -> u64 {
        dir_size(self.dir.path())
    }

    fn live_rows(&mut self) -> usize {
        self.db().metric.all_hot().len()
    }

    fn physical_rows(&mut self) -> Option<usize> {
        Some(self.db().metric.row_count())
    }

    fn reopen(&mut self) -> Option<std::time::Duration> {
        // Flush first, so what is measured is the open path rather than leftover WAL
        // replay from an unclean shutdown — those are different costs and conflating
        // them would overstate the reopen number.
        self.db_mut().checkpoint();
        let root = self.dir.path().to_path_buf();
        drop(self.db.take()); // releases the DirLock
        let t = std::time::Instant::now();
        let db = Database::open_at(root);
        let elapsed = t.elapsed();
        self.db = Some(db);
        Some(elapsed)
    }
}
