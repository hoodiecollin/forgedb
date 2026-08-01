//! `Doc` subject targets — the variable-width mirror of the `Metric` targets (#218).
//!
//! `Metric` isolates `FixedColumn::export`; `Doc` isolates
//! `VariableColumn::gather_buffered`, which ignores the requested indices and reads the
//! **whole** committed offsets index plus the **whole** data region — dead versions
//! included — into owned buffers on every scan. That cost is ~`A x live_bytes`, so where
//! #221 was a *step* (a lost `mmap` fast path, flat in amplification) this should be a
//! *slope*. Telling those two shapes apart is the point: a step is an implementation
//! artifact, a slope is a real cost of keeping old versions.
//!
//! Two ForgeDB flavours are built from one macro:
//!
//! * `compacting` — the default generated build. Auto-compaction caps amplification at
//!   `1 + 4000/live_rows`, so on a 10k-row corpus the whole reachable ladder is
//!   1.0x-1.4x. Useful as the realistic-configuration reference, useless for
//!   establishing a slope.
//! * `unbounded` — the `churn_probe` variant (`compaction = false` + `fsync = "never"`).
//!   Compaction-off lifts the ceiling so the ladder can reach 8x/16x/32x at all;
//!   fsync-never keeps the preload affordable there. Legitimate because this measures
//!   *reads*: durability is an orthogonal axis in the #167 framing, and #218 finding 3
//!   already showed writes are ~100% fsync-bound and identical across engines.

use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};

/// Stable uuid for a `Doc` workload key. Kind tag 5 keeps these clear of the shared
/// corpus (1/2/3) and of `Metric` (4).
pub fn doc_id(key: u64) -> uuid::Uuid {
    uuid::Uuid::from_u128((5u128 << 96) | key as u128)
}

/// Deterministic ASCII payload of **exactly** `n` bytes.
///
/// Varies with `(key, generation, col)` so every update writes genuinely different
/// bytes — an engine that noticed an unchanged value and skipped the write would
/// quietly stop churning, and the amplification ladder would silently measure nothing.
pub fn payload(key: u64, generation: u64, col: u8, n: usize) -> String {
    let mut s = String::with_capacity(n);
    let mut x = key
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ generation.wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (col as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    // Non-zero seed: xorshift64 is stuck at zero.
    x |= 1;
    while s.len() < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        for k in 0..8 {
            if s.len() == n {
                break;
            }
            let b = ((x >> (k * 8)) & 0x1F) as u8;
            s.push(if b < 26 {
                (b'a' + b) as char
            } else {
                (b'0' + (b - 26)) as char
            });
        }
    }
    s
}

/// Stamps out a `Doc` [`WorkloadTarget`] over whichever generated `Database`/`Doc` pair
/// is in scope at the expansion site. The two generated builds are distinct types with
/// identical shape, so a macro is the honest way to avoid duplicating the body — there
/// is no trait over generated models to be generic across.
macro_rules! forge_doc_target {
    ($name:ident, $label:literal) => {
        pub struct $name {
            /// `Option` so `reopen` can drop the live handle first: the data dir is under
            /// an exclusive `DirLock` (#89), so opening a second handle without dropping
            /// the first panics rather than measures.
            db: Option<Database>,
            dir: tempfile::TempDir,
            generation: u64,
            payload_bytes: usize,
        }

        impl $name {
            pub fn new(payload_bytes: usize) -> Self {
                let dir = tempfile::tempdir().unwrap();
                let db = Some(Database::open_at(dir.path().to_path_buf()));
                Self { db, dir, generation: 0, payload_bytes }
            }

            fn db(&self) -> &Database {
                self.db.as_ref().expect("handle is only absent inside reopen")
            }

            fn db_mut(&mut self) -> &mut Database {
                self.db.as_mut().expect("handle is only absent inside reopen")
            }

            fn row(&self, key: u64, g: u64) -> Doc {
                let n = self.payload_bytes;
                Doc {
                    id: doc_id(key),
                    seq: g,
                    kind: (key % 16) as u32,
                    body_a: payload(key, g, 0, n),
                    body_b: payload(key, g, 1, n),
                    body_c: payload(key, g, 2, n),
                    body_d: payload(key, g, 3, n),
                }
            }
        }

        impl WorkloadTarget for $name {
            fn name(&self) -> &'static str {
                $label
            }

            fn create(&mut self, key: u64) -> OpOutcome {
                self.generation += 1;
                let rec = self.row(key, self.generation);
                match self.db_mut().doc.insert(rec) {
                    Ok(_) => OpOutcome::ok(),
                    Err(_) => OpOutcome::miss(),
                }
            }

            fn read(&mut self, key: u64) -> OpOutcome {
                match self.db().doc.get(doc_id(key)) {
                    Some(_) => OpOutcome::ok(),
                    None => OpOutcome::miss(),
                }
            }

            fn update(&mut self, key: u64, width: UpdateWidth) -> OpOutcome {
                self.generation += 1;
                let id = doc_id(key);
                // ForgeDB writes the whole row either way; the widths differ in which
                // bytes actually change. `OneField` touches only the fixed `seq`, so the
                // four string columns re-append byte-identical content — which is itself
                // the point, since the append happens regardless.
                let rec = match width {
                    UpdateWidth::AllFields => self.row(key, self.generation),
                    UpdateWidth::OneField => match self.db().doc.get(id) {
                        Some(mut cur) => {
                            cur.seq = self.generation;
                            cur
                        }
                        None => return OpOutcome::miss(),
                    },
                };
                match self.db_mut().doc.update(id, rec) {
                    Ok(true) => OpOutcome::ok(),
                    _ => OpOutcome::miss(),
                }
            }

            fn delete(&mut self, key: u64) -> OpOutcome {
                if self.db_mut().doc.delete(doc_id(key)) {
                    OpOutcome::ok()
                } else {
                    OpOutcome::miss()
                }
            }

            fn scan(&mut self, kind: ScanKind, limit: usize) -> OpOutcome {
                let n = match kind {
                    // `@projection(meta: seq, kind)` — fixed columns ONLY, so this path
                    // never touches a `VariableColumn`. It is the in-run control: it
                    // should stay flat in amplification after #221, which makes any slope
                    // in `Narrow` attributable to the variable path rather than to
                    // machine state.
                    ScanKind::Projection => self.db().doc.all_meta().len(),
                    // Drags all four string columns through `gather_buffered`.
                    ScanKind::Narrow => self.db().doc.__scan_all().len(),
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
                self.db().doc.all_meta().len()
            }

            fn physical_rows(&mut self) -> Option<usize> {
                Some(self.db().doc.row_count())
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
    };
}

/// The default generated build — auto-compaction on, amplification capped.
pub mod compacting {
    use super::{doc_id, payload};
    use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};
    use forgedb_benchmarks::forgedb_generated::{Database, Doc};

    forge_doc_target!(ForgeDocTarget, "forgedb");
}

/// The `churn_probe` build — compaction off, so amplification is unbounded and a slope
/// can actually be established.
pub mod unbounded {
    use super::{doc_id, payload};
    use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};
    use forgedb_benchmarks::v_churn_probe::{Database, Doc};

    forge_doc_target!(ForgeDocTargetNoCompact, "forgedb-nc");
}

// ---------------------------------------------------------------------------
// SQLite / redb reference lines
// ---------------------------------------------------------------------------

fn key_blob(key: u64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0] = 5; // matches doc_id's kind tag
    b[8..].copy_from_slice(&key.to_be_bytes());
    b
}

/// SQLite over the `doc` table. Same durability parity rules as the `Metric` target:
/// `synchronous = FULL` + `fullfsync = 1`, one autocommit transaction per op.
pub struct SqliteDocTarget {
    conn: rusqlite::Connection,
    dir: tempfile::TempDir,
    generation: u64,
    payload_bytes: usize,
}

impl SqliteDocTarget {
    pub fn new(payload_bytes: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let conn = rusqlite::Connection::open(dir.path().join("bench.db")).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA fullfsync = 1;",
        )
        .unwrap();
        conn.execute_batch(include_str!("../../schema.sql")).unwrap();
        Self { conn, dir, generation: 0, payload_bytes }
    }
}

impl WorkloadTarget for SqliteDocTarget {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn create(&mut self, key: u64) -> OpOutcome {
        self.generation += 1;
        let g = self.generation;
        let n = self.payload_bytes;
        let r = self.conn.execute(
            "INSERT OR REPLACE INTO doc (id, seq, kind, body_a, body_b, body_c, body_d) \
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            rusqlite::params![
                &key_blob(key)[..],
                g as i64,
                (key % 16) as i64,
                payload(key, g, 0, n),
                payload(key, g, 1, n),
                payload(key, g, 2, n),
                payload(key, g, 3, n),
            ],
        );
        if r.is_ok() { OpOutcome::ok() } else { OpOutcome::miss() }
    }

    fn read(&mut self, key: u64) -> OpOutcome {
        let r: Result<i64, _> = self.conn.query_row(
            "SELECT seq FROM doc WHERE id = ?1",
            rusqlite::params![&key_blob(key)[..]],
            |row| row.get(0),
        );
        if r.is_ok() { OpOutcome::ok() } else { OpOutcome::miss() }
    }

    fn update(&mut self, key: u64, width: UpdateWidth) -> OpOutcome {
        self.generation += 1;
        let g = self.generation;
        let n = self.payload_bytes;
        // The in-place advantage made explicit: a one-field update writes one column,
        // where the append-only path re-appends every column regardless.
        let r = match width {
            UpdateWidth::OneField => self.conn.execute(
                "UPDATE doc SET seq = ?2 WHERE id = ?1",
                rusqlite::params![&key_blob(key)[..], g as i64],
            ),
            UpdateWidth::AllFields => self.conn.execute(
                "UPDATE doc SET seq = ?2, kind = ?3, body_a = ?4, body_b = ?5, body_c = ?6, \
                 body_d = ?7 WHERE id = ?1",
                rusqlite::params![
                    &key_blob(key)[..],
                    g as i64,
                    (key % 16) as i64,
                    payload(key, g, 0, n),
                    payload(key, g, 1, n),
                    payload(key, g, 2, n),
                    payload(key, g, 3, n),
                ],
            ),
        };
        match r {
            Ok(rows) if rows > 0 => OpOutcome::ok(),
            _ => OpOutcome::miss(),
        }
    }

    fn delete(&mut self, key: u64) -> OpOutcome {
        match self
            .conn
            .execute("DELETE FROM doc WHERE id = ?1", rusqlite::params![&key_blob(key)[..]])
        {
            Ok(rows) if rows > 0 => OpOutcome::ok(),
            _ => OpOutcome::miss(),
        }
    }

    fn scan(&mut self, kind: ScanKind, limit: usize) -> OpOutcome {
        // Mirrors ForgeDB's column sets exactly: the projection reads the two fixed
        // columns, the narrow scan reads all four TEXT bodies.
        let sql = match kind {
            ScanKind::Projection => "SELECT seq, kind FROM doc",
            ScanKind::Narrow => "SELECT id, seq, kind, body_a, body_b, body_c, body_d FROM doc",
        };
        let mut stmt = self.conn.prepare_cached(sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut n = 0u64;
        while let Some(row) = rows.next().unwrap() {
            if matches!(kind, ScanKind::Narrow) {
                // Materialize the strings; leaving them unread would compare a
                // full materialization against a column-offset walk.
                let _: Vec<u8> = row.get(0).unwrap();
                for i in 3..7 {
                    let _: String = row.get(i).unwrap();
                }
            } else {
                let _: i64 = row.get(0).unwrap();
                let _: i64 = row.get(1).unwrap();
            }
            n += 1;
        }
        OpOutcome::rows(n.min(limit as u64))
    }

    fn maintain(&mut self) {
        self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
    }

    fn footprint(&self) -> u64 {
        dir_size(self.dir.path())
    }

    fn live_rows(&mut self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM doc", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }
}
