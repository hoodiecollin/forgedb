//! redb [`WorkloadTarget`] — a second in-place reference, B-tree/copy-on-write.
//!
//! Included because SQLite alone is a weak control: if ForgeDB trails SQLite under
//! churn, a single data point cannot distinguish "the append-only model costs this"
//! from "SQLite is simply very good at this". redb is an embedded pure-Rust key-value
//! store with a different internal design, so agreement between the two in-place
//! engines is real evidence about the model rather than about one implementation.
//!
//! `Durability::Immediate` per commit, one commit per operation — the same one-barrier-
//! per-op contract ForgeDB's `FsyncPolicy::Always` provides.

use redb::{Database as Redb, Durability, ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};

const METRIC: TableDefinition<&[u8], &[u8]> = TableDefinition::new("metric");

pub struct RedbTarget {
    db: Redb,
    dir: tempfile::TempDir,
    generation: u64,
}

fn key_blob(key: u64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0] = 4;
    b[8..].copy_from_slice(&key.to_be_bytes());
    b
}

/// The 22 columns packed as one fixed-layout value. Byte-for-byte the same field set
/// the other engines store; only the layout differs, which is exactly the difference
/// under comparison.
fn pack(key: u64, g: u64) -> Vec<u8> {
    let m = key.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ g.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let mut v = Vec::with_capacity(128);
    v.extend_from_slice(&(1_700_000_000i64 + (key as i64 % 100_000)).to_le_bytes());
    v.extend_from_slice(&(key % 1024).to_le_bytes());
    v.extend_from_slice(&g.to_le_bytes());
    v.extend_from_slice(&((m % 16) as u32).to_le_bytes());
    for f in [
        (m % 10_000) as f64 / 100.0,
        ((m >> 8) % 10_000) as f64 / 100.0,
        ((m >> 16) % 10_000) as f64 / 100.0,
    ] {
        v.extend_from_slice(&f.to_le_bytes());
    }
    for u in [
        m % 1_000_000,
        (m >> 4) % 1_000_000,
        (m >> 8) % 100_000,
        (m >> 12) % 1_000,
    ] {
        v.extend_from_slice(&u.to_le_bytes());
    }
    for u in [
        ((m >> 16) % 10_000) as u32,
        ((m >> 20) % 50_000) as u32,
        ((m >> 24) % 100_000) as u32,
        ((m >> 28) % 512) as u32,
        ((m >> 32) % 4096) as u32,
        ((m >> 36) % 20_000) as u32,
    ] {
        v.extend_from_slice(&u.to_le_bytes());
    }
    v.extend_from_slice(&((m % 10_000_000) as i64).to_le_bytes());
    v.extend_from_slice(&(20.0 + ((m >> 40) % 6000) as f64 / 100.0).to_le_bytes());
    v.push((m & 1) as u8);
    v.push(((m & 2) == 0) as u8);
    v
}

impl RedbTarget {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Redb::create(dir.path().join("bench.redb")).unwrap();
        // Materialize the table so later read txns never hit a missing-table error.
        let mut tx = db.begin_write().unwrap();
        tx.set_durability(Durability::Immediate);
        {
            let _ = tx.open_table(METRIC).unwrap();
        }
        tx.commit().unwrap();
        Self { db, dir, generation: 0 }
    }

    fn put(&mut self, key: u64, g: u64) -> OpOutcome {
        let mut tx = self.db.begin_write().unwrap();
        tx.set_durability(Durability::Immediate);
        {
            let mut t = tx.open_table(METRIC).unwrap();
            if t.insert(&key_blob(key)[..], pack(key, g).as_slice()).is_err() {
                return OpOutcome::miss();
            }
        }
        match tx.commit() {
            Ok(_) => OpOutcome::ok(),
            Err(_) => OpOutcome::miss(),
        }
    }
}

impl WorkloadTarget for RedbTarget {
    fn name(&self) -> &'static str {
        "redb"
    }

    fn create(&mut self, key: u64) -> OpOutcome {
        self.generation += 1;
        self.put(key, self.generation)
    }

    fn read(&mut self, key: u64) -> OpOutcome {
        let tx = self.db.begin_read().unwrap();
        let t = tx.open_table(METRIC).unwrap();
        match t.get(&key_blob(key)[..]) {
            Ok(Some(_)) => OpOutcome::ok(),
            _ => OpOutcome::miss(),
        }
    }

    // A key-value store has no notion of a partial-row write: the value is one opaque
    // blob, so both widths rewrite it. That is itself informative — it marks the
    // update-width axis as inapplicable here rather than free.
    fn update(&mut self, key: u64, _width: UpdateWidth) -> OpOutcome {
        self.generation += 1;
        self.put(key, self.generation)
    }

    fn delete(&mut self, key: u64) -> OpOutcome {
        let mut tx = self.db.begin_write().unwrap();
        tx.set_durability(Durability::Immediate);
        let existed = {
            let mut t = tx.open_table(METRIC).unwrap();
            matches!(t.remove(&key_blob(key)[..]), Ok(Some(_)))
        };
        let _ = tx.commit();
        if existed { OpOutcome::ok() } else { OpOutcome::miss() }
    }

    fn scan(&mut self, _kind: ScanKind, limit: usize) -> OpOutcome {
        // Row-oriented: a scan must walk whole values whatever columns are wanted, so
        // Projection and Narrow are the same operation. Column pruning is precisely
        // the advantage a columnar layout has here, and #167 keeps columnar either
        // way — so this is a fair asymmetry, not an unfair one.
        let tx = self.db.begin_read().unwrap();
        let t = tx.open_table(METRIC).unwrap();
        let mut n = 0u64;
        if let Ok(iter) = t.iter() {
            for e in iter.flatten() {
                let _ = e.1.value().len();
                n += 1;
            }
        }
        OpOutcome::rows(n.min(limit as u64))
    }

    fn maintain(&mut self) {
        let _ = self.db.compact();
    }

    fn footprint(&self) -> u64 {
        dir_size(self.dir.path())
    }

    fn live_rows(&mut self) -> usize {
        let tx = self.db.begin_read().unwrap();
        let t = tx.open_table(METRIC).unwrap();
        t.len().unwrap_or(0) as usize
    }
}
