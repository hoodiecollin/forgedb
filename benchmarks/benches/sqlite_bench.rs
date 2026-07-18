//! SQLite benchmark suite (rusqlite, bundled). Mirrors the ForgeDB suite's
//! scenarios over the same seeded corpus and the 1:1 `schema.sql` DDL, so the
//! Criterion groups line up for direct comparison. Durability is matched to
//! ForgeDB's `FsyncPolicy::Always`: WAL journal + `synchronous = FULL`.

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use forgedb_benchmarks::{dataset, id_for, Dataset};
use rusqlite::{params, Connection};

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

const SCHEMA: &str = include_str!("../schema.sql");

/// WAL journal + `synchronous = FULL`. `fullfsync` decides the *strength* of the
/// flush, which is the whole fsync-parity question (see docs/BENCHMARKS.md):
///   - `false` → SQLite's default `fsync()`. On macOS that flushes to the drive
///     cache only (~113 µs) — NOT a barrier, a weaker guarantee than ForgeDB.
///   - `true`  → `fcntl(F_FULLFSYNC)` (~4 ms), the same true barrier ForgeDB's
///     WAL `sync_all()` issues on macOS. This is the like-for-like durability.
fn apply_pragmas(conn: &Connection, fullfsync: bool) {
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "FULL").unwrap();
    if fullfsync {
        conn.pragma_update(None, "fullfsync", "1").unwrap();
    }
}

fn fresh_conn_dur(path: &std::path::Path, fullfsync: bool) -> Connection {
    let conn = Connection::open(path).expect("open sqlite");
    apply_pragmas(&conn, fullfsync);
    conn.execute_batch(SCHEMA).expect("apply schema");
    conn
}

/// Default durability (plain fsync). Reads don't touch the fsync path, so the
/// read/bulk fixtures use this.
fn fresh_conn(path: &std::path::Path) -> Connection {
    fresh_conn_dur(path, false)
}

/// Load `data` into `conn` in one transaction (setup — not timed).
fn load(conn: &mut Connection, data: &Dataset) {
    let tx = conn.transaction().unwrap();
    for u in &data.users {
        tx.execute(
            "INSERT INTO user (id, name, email, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![&u.id[..], u.name, u.email, u.created_at],
        )
        .unwrap();
    }
    for p in &data.posts {
        tx.execute(
            "INSERT INTO post (id, title, views, published, author, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&p.id[..], p.title, p.views as i64, p.published as i64, &p.author[..], p.created_at],
        )
        .unwrap();
    }
    for t in &data.tags {
        tx.execute("INSERT INTO tag (id, name) VALUES (?1, ?2)", params![&t.id[..], t.name])
            .unwrap();
    }
    for &(p, t) in &data.links {
        tx.execute(
            "INSERT INTO post_tag_link (post_id, tag_id) VALUES (?1, ?2)",
            params![&data.posts[p].id[..], &data.tags[t].id[..]],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

// --- Scenario 2: single-row insert latency (autocommit → fsync per row) ------
// Measured at BOTH durability levels so the fsync-parity comparison is explicit:
// `default` is SQLite out-of-box (plain fsync), `fullfsync` matches ForgeDB's
// per-commit F_FULLFSYNC barrier. See docs/BENCHMARKS.md.
fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite/insert_user");
    group.throughput(Throughput::Elements(1));
    for &fullfsync in &[false, true] {
        let dir = tempfile::tempdir().unwrap();
        // `dir`/`conn` are locals that live until the end of this loop body,
        // which is after `bench_function` returns (it measures synchronously).
        let conn = fresh_conn_dur(&dir.path().join("bench.db"), fullfsync);
        let mut stmt_i = 0usize;
        let label = if fullfsync { "fullfsync" } else { "default" };
        group.bench_function(label, |b| {
            b.iter_batched(
                || {
                    let i = stmt_i;
                    stmt_i += 1;
                    i
                },
                |i| {
                    let id = uuid::Uuid::from_u128(
                        0xF000_0000_0000_0000_0000_0000_0000_0000 + i as u128,
                    )
                    .into_bytes();
                    conn.execute(
                        "INSERT INTO user (id, name, email, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![&id[..], "bulk", format!("insert{i}@example.com"), 1_700_000_000i64],
                    )
                    .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// --- Scenario 1: bulk load (autocommit → fsync per row, matching ForgeDB) ----
fn bench_bulk_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite/bulk_load_posts");
    group.sample_size(10);
    for &n in &[1_000usize, 10_000] {
        let data = dataset(n.min(2_000).max(1), n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || tempfile::tempdir().unwrap(),
                |dir| {
                    let conn = fresh_conn(&dir.path().join("bench.db"));
                    for u in &data.users {
                        conn.execute(
                            "INSERT INTO user (id, name, email, created_at) VALUES (?1, ?2, ?3, ?4)",
                            params![&u.id[..], u.name, u.email, u.created_at],
                        )
                        .unwrap();
                    }
                    for p in &data.posts {
                        conn.execute(
                            "INSERT INTO post (id, title, views, published, author, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![&p.id[..], p.title, p.views as i64, p.published as i64, &p.author[..], p.created_at],
                        )
                        .unwrap();
                    }
                    dir
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

// --- Read / traversal scenarios (5, 6, 8/10, 11) -----------------------------
fn bench_reads(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let mut conn = fresh_conn(&dir.path().join("bench.db"));
    load(&mut conn, &data);

    // Full-row materializers so SQLite builds the SAME records ForgeDB's
    // get/user_posts/post_tags return — otherwise `SELECT id` would flatter
    // SQLite by skipping the column reads ForgeDB pays for.
    type PostRowOut = (Vec<u8>, String, i64, i64, Vec<u8>, i64);
    type UserRowOut = (Vec<u8>, String, String, i64);
    type TagRowOut = (Vec<u8>, String);

    // Scenario 5: point lookup by PK (materialize the full post record).
    c.benchmark_group("sqlite/point_lookup")
        .throughput(Throughput::Elements(1))
        .bench_function("get_post_by_id", |b| {
            let mut stmt = conn
                .prepare("SELECT id, title, views, published, author, created_at FROM post WHERE id = ?1")
                .unwrap();
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(2, i % READ_POSTS);
                i += 1;
                let row: Option<PostRowOut> = stmt
                    .query_row(params![&id[..]], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                    })
                    .ok();
                std::hint::black_box(row)
            });
        });

    // Scenario 6: secondary-index probe (unique email → full user record).
    c.benchmark_group("sqlite/index_probe")
        .throughput(Throughput::Elements(1))
        .bench_function("get_user_by_email", |b| {
            let mut stmt = conn
                .prepare("SELECT id, name, email, created_at FROM user WHERE email = ?1")
                .unwrap();
            let mut i = 0usize;
            b.iter(|| {
                let email = format!("user{}@example.com", i % READ_USERS);
                i += 1;
                let row: Option<UserRowOut> = stmt
                    .query_row(params![email], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .ok();
                std::hint::black_box(row)
            });
        });

    // Scenario 8/10: FK-index probe → reverse one-to-many (full post records).
    c.benchmark_group("sqlite/reverse_fk")
        .throughput(Throughput::Elements(1))
        .bench_function("user_posts", |b| {
            let mut stmt = conn
                .prepare("SELECT id, title, views, published, author, created_at FROM post WHERE author = ?1")
                .unwrap();
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(1, i % READ_USERS);
                i += 1;
                let rows: Vec<PostRowOut> = stmt
                    .query_map(params![&id[..]], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                    })
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                std::hint::black_box(rows)
            });
        });

    // Scenario 11: many-to-many traversal (indexed junction join → full tags).
    c.benchmark_group("sqlite/m2m")
        .throughput(Throughput::Elements(1))
        .bench_function("post_tags", |b| {
            let mut stmt = conn
                .prepare(
                    "SELECT tag.id, tag.name FROM tag \
                     JOIN post_tag_link l ON l.tag_id = tag.id WHERE l.post_id = ?1",
                )
                .unwrap();
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(2, i % READ_POSTS);
                i += 1;
                let rows: Vec<TagRowOut> = stmt
                    .query_map(params![&id[..]], |r| Ok((r.get(0)?, r.get(1)?)))
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                std::hint::black_box(rows)
            });
        });
}

criterion_group!(benches, bench_insert, bench_bulk_load, bench_reads);
criterion_main!(benches);
