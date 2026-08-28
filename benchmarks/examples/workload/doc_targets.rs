use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};

pub fn doc_id(key: u64) -> uuid::Uuid {
    uuid::Uuid::from_u128((5u128 << 96) | key as u128)
}

pub fn payload(key: u64, generation: u64, col: u8, n: usize) -> String {
    let mut s = String::with_capacity(n);
    let mut x = key
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ generation.wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (col as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
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

macro_rules! forge_doc_target {
    ($name:ident, $label:literal) => {
        pub struct $name {
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
                    ScanKind::Projection => self.db().doc.all_meta().len(),
                    ScanKind::Narrow => self
                        .db()
                        .doc
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

pub mod compacting {
    use super::{doc_id, payload};
    use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};
    use forgedb_benchmarks::forgedb_generated::{Database, Doc};

    forge_doc_target!(ForgeDocTarget, "forgedb");
}

pub mod unbounded {
    use super::{doc_id, payload};
    use crate::driver::{dir_size, OpOutcome, ScanKind, UpdateWidth, WorkloadTarget};
    use forgedb_benchmarks::v_churn_probe::{Database, Doc};

    forge_doc_target!(ForgeDocTargetNoCompact, "forgedb-nc");
}

fn key_blob(key: u64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0] = 5;
    b[8..].copy_from_slice(&key.to_be_bytes());
    b
}

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
        let sql = match kind {
            ScanKind::Projection => "SELECT seq, kind FROM doc",
            ScanKind::Narrow => "SELECT id, seq, kind, body_a, body_b, body_c, body_d FROM doc",
        };
        let mut stmt = self.conn.prepare_cached(sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut n = 0u64;
        while let Some(row) = rows.next().unwrap() {
            if matches!(kind, ScanKind::Narrow) {
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
