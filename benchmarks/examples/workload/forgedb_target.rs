use forgedb_benchmarks::forgedb_generated::{Database, Metric};
use forgedb_benchmarks::ts_from_seconds;
use uuid::Uuid;

use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};

pub fn metric_id(key: u64) -> Uuid {
    Uuid::from_u128((4u128 << 96) | key as u128)
}

pub struct ForgeTarget {
    db: Option<Database>,
    dir: tempfile::TempDir,
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
            ScanKind::Projection => self.db().metric.all_hot().len(),
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
        self.db_mut().checkpoint();
        let root = self.dir.path().to_path_buf();
        drop(self.db.take());
        let t = std::time::Instant::now();
        let db = Database::open_at(root);
        let elapsed = t.elapsed();
        self.db = Some(db);
        Some(elapsed)
    }
}
