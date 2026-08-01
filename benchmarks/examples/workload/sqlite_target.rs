//! SQLite [`WorkloadTarget`] — the update-in-place baseline.
//!
//! **Durability parity is the whole point of this target and is not negotiable.**
//! ForgeDB's generated write path WAL-records every mutation under
//! `FsyncPolicy::Always`, i.e. one durability barrier per operation. So SQLite runs
//! `synchronous = FULL` with `fullfsync = 1` (on Apple hardware a plain fsync does not
//! reach the platter, and comparing a real barrier against a lying one would be the
//! single easiest way to produce a flattering, meaningless number), and each operation
//! is its own autocommit transaction rather than batched.
//!
//! Its value here is as the honest in-place reference: whatever curve SQLite traces
//! under the same churn is roughly what a ForgeDB in-place variant could hope to
//! reproduce. A ForgeDB-vs-SQLite gap that closes after compaction says the append tax
//! is bounded; one that widens with amplification is the investment case for #172.

use rusqlite::{params, Connection};

use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};

pub struct SqliteTarget {
    conn: Connection,
    dir: tempfile::TempDir,
    generation: u64,
}

fn key_blob(key: u64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0] = 4; // kind tag, matching the ForgeDB target's uuid layout
    b[8..].copy_from_slice(&key.to_be_bytes());
    b
}

impl SqliteTarget {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("bench.db")).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA fullfsync = 1;",
        )
        .unwrap();
        conn.execute_batch(include_str!("../../schema.sql")).unwrap();
        Self { conn, dir, generation: 0 }
    }
}

const COLS: &str = "id, recorded_at, device_id, sample_seq, region, cpu_pct, mem_pct, disk_pct, \
                    net_rx_bytes, net_tx_bytes, req_count, err_count, p50_micros, p95_micros, \
                    p99_micros, queue_depth, open_conns, gc_pause_micros, uptime_secs, \
                    temp_celsius, throttled, healthy";

impl WorkloadTarget for SqliteTarget {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn create(&mut self, key: u64) -> OpOutcome {
        self.generation += 1;
        let g = self.generation;
        let m = key.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ g.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let r = self.conn.execute(
            &format!(
                "INSERT OR REPLACE INTO metric ({COLS}) VALUES \
                 (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)"
            ),
            params![
                &key_blob(key)[..],
                1_700_000_000i64 + (key as i64 % 100_000),
                (key % 1024) as i64,
                g as i64,
                (m % 16) as i64,
                (m % 10_000) as f64 / 100.0,
                ((m >> 8) % 10_000) as f64 / 100.0,
                ((m >> 16) % 10_000) as f64 / 100.0,
                (m % 1_000_000) as i64,
                ((m >> 4) % 1_000_000) as i64,
                ((m >> 8) % 100_000) as i64,
                ((m >> 12) % 1_000) as i64,
                ((m >> 16) % 10_000) as i64,
                ((m >> 20) % 50_000) as i64,
                ((m >> 24) % 100_000) as i64,
                ((m >> 28) % 512) as i64,
                ((m >> 32) % 4096) as i64,
                ((m >> 36) % 20_000) as i64,
                (m % 10_000_000) as i64,
                20.0 + ((m >> 40) % 6000) as f64 / 100.0,
                (m & 1) as i64,
                ((m & 2) == 0) as i64,
            ],
        );
        match r {
            Ok(_) => OpOutcome::ok(),
            Err(_) => OpOutcome::miss(),
        }
    }

    fn read(&mut self, key: u64) -> OpOutcome {
        let got: Result<i64, _> = self.conn.query_row(
            "SELECT sample_seq FROM metric WHERE id = ?1",
            params![&key_blob(key)[..]],
            |r| r.get(0),
        );
        if got.is_ok() { OpOutcome::ok() } else { OpOutcome::miss() }
    }

    fn update(&mut self, key: u64, width: UpdateWidth) -> OpOutcome {
        self.generation += 1;
        let g = self.generation;
        let n = match width {
            // The in-place engine's advantage made explicit: it can touch one column.
            UpdateWidth::OneField => self.conn.execute(
                "UPDATE metric SET sample_seq = ?2 WHERE id = ?1",
                params![&key_blob(key)[..], g as i64],
            ),
            // Full-row rewrite — the shape ForgeDB is forced into on every update.
            UpdateWidth::AllFields => {
                let m = key.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ g.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                self.conn.execute(
                    "UPDATE metric SET recorded_at=?2, device_id=?3, sample_seq=?4, region=?5, \
                     cpu_pct=?6, mem_pct=?7, disk_pct=?8, net_rx_bytes=?9, net_tx_bytes=?10, \
                     req_count=?11, err_count=?12, p50_micros=?13, p95_micros=?14, p99_micros=?15, \
                     queue_depth=?16, open_conns=?17, gc_pause_micros=?18, uptime_secs=?19, \
                     temp_celsius=?20, throttled=?21, healthy=?22 WHERE id=?1",
                    params![
                        &key_blob(key)[..],
                        1_700_000_000i64 + (key as i64 % 100_000),
                        (key % 1024) as i64,
                        g as i64,
                        (m % 16) as i64,
                        (m % 10_000) as f64 / 100.0,
                        ((m >> 8) % 10_000) as f64 / 100.0,
                        ((m >> 16) % 10_000) as f64 / 100.0,
                        (m % 1_000_000) as i64,
                        ((m >> 4) % 1_000_000) as i64,
                        ((m >> 8) % 100_000) as i64,
                        ((m >> 12) % 1_000) as i64,
                        ((m >> 16) % 10_000) as i64,
                        ((m >> 20) % 50_000) as i64,
                        ((m >> 24) % 100_000) as i64,
                        ((m >> 28) % 512) as i64,
                        ((m >> 32) % 4096) as i64,
                        ((m >> 36) % 20_000) as i64,
                        (m % 10_000_000) as i64,
                        20.0 + ((m >> 40) % 6000) as f64 / 100.0,
                        (m & 1) as i64,
                        ((m & 2) == 0) as i64,
                    ],
                )
            }
        };
        match n {
            Ok(0) => OpOutcome::miss(),
            Ok(_) => OpOutcome::ok(),
            Err(_) => OpOutcome::miss(),
        }
    }

    fn delete(&mut self, key: u64) -> OpOutcome {
        match self.conn.execute("DELETE FROM metric WHERE id = ?1", params![&key_blob(key)[..]]) {
            Ok(0) => OpOutcome::miss(),
            Ok(_) => OpOutcome::ok(),
            Err(_) => OpOutcome::miss(),
        }
    }

    fn scan(&mut self, kind: ScanKind, limit: usize) -> OpOutcome {
        // Mirrors the ForgeDB scan's column set exactly: the projection reads two
        // columns, the narrow scan reads the filterable/sortable set. Letting SQLite
        // read fewer columns than ForgeDB would hand it a free win on a columnar
        // engine's home turf.
        let sql = match kind {
            ScanKind::Projection => "SELECT id, cpu_pct, mem_pct FROM metric",
            ScanKind::Narrow => "SELECT id, recorded_at, device_id, sample_seq, region, cpu_pct, \
                                 mem_pct, disk_pct, net_rx_bytes, net_tx_bytes, req_count, \
                                 err_count, p50_micros, p95_micros, p99_micros, queue_depth, \
                                 open_conns, gc_pause_micros, uptime_secs, temp_celsius, \
                                 throttled, healthy FROM metric",
        };
        let mut stmt = self.conn.prepare_cached(sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut n = 0u64;
        while let Ok(Some(r)) = rows.next() {
            // Touch a column so the read is not optimized into a pure row count.
            let _: Vec<u8> = r.get(0).unwrap_or_default();
            n += 1;
        }
        OpOutcome::rows(n.min(limit as u64))
    }

    fn maintain(&mut self) {
        // SQLite's analogue of compaction: fold the WAL back and reclaim free pages.
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
    }

    fn footprint(&self) -> u64 {
        dir_size(self.dir.path())
    }

    fn live_rows(&mut self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM metric", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    // No version chain: an UPDATE rewrites the row. Reporting an amplification here
    // would invent a quantity the engine does not have.
    fn physical_rows(&mut self) -> Option<usize> {
        None
    }
}
