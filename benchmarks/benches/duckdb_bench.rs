//! DuckDB benchmark suite (embedded, columnar/vectorized). Mirrors the ForgeDB /
//! SQLite / redb scenarios over the SAME seeded corpus so the Criterion groups line
//! up. DuckDB is here for its *storage model* (vectorized columnar, like ForgeDB's
//! columns) — it is expected to WIN scan/aggregate workloads and to be comparatively
//! WEAK at single-row OLTP (point insert / point lookup), because a columnar engine
//! optimizes bulk vectorized access, not row-at-a-time. Showing that contrast is the
//! point, not a failure. Reads materialize the full record (SELECT every column),
//! matching the other suites.
//!
//! Durability note: DuckDB is an analytical store with a WAL + checkpoint; it has no
//! per-row `synchronous=FULL`/`fullfsync` OLTP knob, so its single-insert latency
//! reflects its own WAL/checkpoint model — NOT a matched `F_FULLFSYNC` barrier. Its
//! write numbers are labeled `duckdb` and are not a like-for-like barrier comparison.

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use duckdb::{params, Connection};
use forgedb_benchmarks::{
    dataset, id_for, list_sql, ts_from_seconds, uuid_of, Dataset, PostJson, LIST_CORE_LIMIT,
    LIST_CORE_ROWS, LIST_LIMITS, LIST_SHAPES, LIST_SIZES,
};

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

const SCHEMA: &str = r#"
-- Index parity with `bench.forge` / `schema.sql` (#282 BDD-10). The contract stated at
-- the top of schema.sql is "every ForgeDB index has a matching SQL index so index-probe
-- scenarios compare like for like", and this DDL silently broke it for TWO of the five:
-- `^views` on Post and `&name` on Tag. Both are ADDED here rather than worked around,
-- because #282's `filtered_indexed` shape probes `views` directly -- an unindexed engine
-- would have posted a full scan under a benchmark ID that reads as an index lookup.
--
-- This makes this suite's bulk-load and insert numbers SLOWER than previously published:
-- it was paying for one fewer index than SQLite and ForgeDB. The old numbers were the
-- unfair ones. Recorded in docs/BENCHMARKS.md rather than quietly re-baselined.
CREATE TABLE user (id BLOB PRIMARY KEY, name VARCHAR, email VARCHAR UNIQUE, created_at BIGINT);
CREATE TABLE post (id BLOB PRIMARY KEY, title VARCHAR, views UBIGINT, published BOOLEAN, author BLOB, created_at BIGINT);
CREATE INDEX post_author_idx ON post(author);
CREATE INDEX post_views_idx ON post(views);
CREATE TABLE tag (id BLOB PRIMARY KEY, name VARCHAR UNIQUE);
CREATE TABLE post_tag_link (post_id BLOB, tag_id BLOB);
CREATE INDEX ptl_post_idx ON post_tag_link(post_id);
"#;

fn fresh_conn(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).expect("open duckdb");
    conn.execute_batch(SCHEMA).expect("apply schema");
    conn
}

/// Load `data` into `conn` inside one transaction (setup — not timed).
fn load(conn: &Connection, data: &Dataset) {
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for u in &data.users {
        conn.execute(
            "INSERT INTO user (id, name, email, created_at) VALUES (?, ?, ?, ?)",
            params![&u.id[..], u.name, u.email, u.created_at],
        )
        .unwrap();
    }
    for p in &data.posts {
        conn.execute(
            "INSERT INTO post (id, title, views, published, author, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            params![&p.id[..], p.title, p.views, p.published, &p.author[..], p.created_at],
        )
        .unwrap();
    }
    for t in &data.tags {
        conn.execute("INSERT INTO tag (id, name) VALUES (?, ?)", params![&t.id[..], t.name]).unwrap();
    }
    for &(p, t) in &data.links {
        conn.execute(
            "INSERT INTO post_tag_link (post_id, tag_id) VALUES (?, ?)",
            params![&data.posts[p].id[..], &data.tags[t].id[..]],
        )
        .unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();
}

// --- Scenario 2: single-row insert latency (autocommit) ----------------------
fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("duckdb/insert_user");
    group.throughput(Throughput::Elements(1));
    let dir = tempfile::tempdir().unwrap();
    let conn = fresh_conn(&dir.path().join("bench.duckdb"));
    let mut i = 0usize;
    group.bench_function("duckdb", |b| {
        b.iter_batched(
            || {
                let n = i;
                i += 1;
                n
            },
            |n| {
                let id = uuid::Uuid::from_u128(0xF000_0000_0000_0000_0000_0000_0000_0000 + n as u128)
                    .into_bytes();
                conn.execute(
                    "INSERT INTO user (id, name, email, created_at) VALUES (?, ?, ?, ?)",
                    params![&id[..], "bulk", format!("insert{n}@example.com"), 1_700_000_000i64],
                )
                .unwrap();
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

// --- Scenario 1: bulk load (single-row INSERTs in one txn) -------------------
// Row-at-a-time INSERT is a columnar engine's worst case (its bulk path is the
// Appender API). We use per-row INSERT for cross-engine parity; DuckDB's own bulk
// throughput would be measured separately with the Appender.
fn bench_bulk_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("duckdb/bulk_load_posts");
    group.sample_size(10);
    for &n in &[1_000usize, 10_000] {
        let data = dataset(n.min(2_000).max(1), n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || tempfile::tempdir().unwrap(),
                |dir| {
                    let conn = fresh_conn(&dir.path().join("bench.duckdb"));
                    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
                    for u in &data.users {
                        conn.execute(
                            "INSERT INTO user (id, name, email, created_at) VALUES (?, ?, ?, ?)",
                            params![&u.id[..], u.name, u.email, u.created_at],
                        )
                        .unwrap();
                    }
                    for p in &data.posts {
                        conn.execute(
                            "INSERT INTO post (id, title, views, published, author, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                            params![&p.id[..], p.title, p.views, p.published, &p.author[..], p.created_at],
                        )
                        .unwrap();
                    }
                    conn.execute_batch("COMMIT;").unwrap();
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
    let conn = fresh_conn(&dir.path().join("bench.duckdb"));
    load(&conn, &data);

    type PostRowOut = (Vec<u8>, String, u64, bool, Vec<u8>, i64);
    type UserRowOut = (Vec<u8>, String, String, i64);
    type TagRowOut = (Vec<u8>, String);

    // Scenario 5: point lookup by PK.
    c.benchmark_group("duckdb/point_lookup")
        .throughput(Throughput::Elements(1))
        .bench_function("get_post_by_id", |b| {
            let mut stmt = conn
                .prepare("SELECT id, title, views, published, author, created_at FROM post WHERE id = ?")
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

    // Scenario 6: secondary-index probe (unique email).
    c.benchmark_group("duckdb/index_probe")
        .throughput(Throughput::Elements(1))
        .bench_function("get_user_by_email", |b| {
            let mut stmt = conn
                .prepare("SELECT id, name, email, created_at FROM user WHERE email = ?")
                .unwrap();
            let mut i = 0usize;
            b.iter(|| {
                let email = format!("user{}@example.com", i % READ_USERS);
                i += 1;
                let row: Option<UserRowOut> = stmt
                    .query_row(params![email], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .ok();
                std::hint::black_box(row)
            });
        });

    // Scenario 8/10: FK-index probe -> reverse one-to-many.
    c.benchmark_group("duckdb/reverse_fk")
        .throughput(Throughput::Elements(1))
        .bench_function("user_posts", |b| {
            let mut stmt = conn
                .prepare("SELECT id, title, views, published, author, created_at FROM post WHERE author = ?")
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

    // Scenario 11: many-to-many traversal (indexed junction join).
    c.benchmark_group("duckdb/m2m")
        .throughput(Throughput::Elements(1))
        .bench_function("post_tags", |b| {
            let mut stmt = conn
                .prepare(
                    "SELECT tag.id, tag.name FROM tag \
                     JOIN post_tag_link l ON l.tag_id = tag.id WHERE l.post_id = ?",
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

// --- Scenario 7: filtered scan + aggregate + top-N ---------------------------
// This is the scenario a columnar/vectorized engine is BUILT to win — a full-table
// scan pruned to the aggregated columns, and a vectorized top-N — the mirror image
// of DuckDB's single-row-OLTP weakness above.
fn bench_scan(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let conn = fresh_conn(&dir.path().join("bench.duckdb"));
    load(&conn, &data);

    // 7a: full scan + aggregate (CAST the UBIGINT SUM back to BIGINT to read as i64).
    c.benchmark_group("duckdb/scan_aggregate")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("sum_views_where_published", |b| {
            let mut stmt = conn
                .prepare("SELECT COUNT(*), CAST(COALESCE(SUM(views), 0) AS BIGINT) FROM post WHERE published")
                .unwrap();
            b.iter(|| {
                let row: (i64, i64) = stmt.query_row([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
                std::hint::black_box(row)
            });
        });

    // 7b: filtered scan + sort + page (top-10 by views, full records).
    type PostRowOut = (Vec<u8>, String, u64, bool, Vec<u8>, i64);
    c.benchmark_group("duckdb/scan_sort_top10")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("top10_by_views", |b| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, views, published, author, created_at FROM post \
                     WHERE views >= ? ORDER BY views DESC LIMIT 10",
                )
                .unwrap();
            b.iter(|| {
                let rows: Vec<PostRowOut> = stmt
                    .query_map(params![50_000u64], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                    })
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                std::hint::black_box(rows)
            });
        });
}


// --- Scenario 21 (#282): the REST list endpoint, S1 and S2 -------------------
//
// S1 = the page's rows in the serializable host-language form this engine's shipped read
// path produces; S2 = the JSON array over the same field set, so `S2 - S1` is the same
// added work in every suite. See `PostJson` and docs/BENCHMARKS.md.
//
// DuckDB is the interesting arm here: like ForgeDB it is columnar, so a 50-row page is a
// vectorized engine being asked to do the one thing it is not built for. Its `filtered_indexed`
// cell is now honest -- `post_views_idx` exists as of this issue (see SCHEMA).

/// Decode the page into `PostJson`. DuckDB hands BLOBs back as `Vec<u8>` and UBIGINT as
/// `u64`, so this is the same materialization the other SQL suites do.
fn duck_page(stmt: &mut duckdb::Statement<'_>) -> Vec<PostJson> {
    stmt.query_map([], |r| {
        Ok(PostJson {
            id: uuid_of(r.get(0)?),
            title: r.get(1)?,
            views: r.get(2)?,
            published: r.get(3)?,
            author: uuid_of(r.get(4)?),
            created_at: ts_from_seconds(r.get(5)?),
            tags: (),
        })
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

fn bench_list(c: &mut Criterion) {
    // The core grid: four shapes at the core point.
    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        let dir = tempfile::tempdir().unwrap();
        let conn = fresh_conn(&dir.path().join("bench.duckdb"));
        load(&conn, &data);
        let mut g = c.benchmark_group("duckdb/list_core");
        for (name, clause) in LIST_SHAPES {
            let limit = LIST_CORE_LIMIT;
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={LIST_CORE_ROWS}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    std::hint::black_box(duck_page(&mut stmt).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    let page = duck_page(&mut stmt);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    // Size sweep, unfiltered. The core point recurs here on purpose — it is the sweep's
    // middle point — which is legal because this is a DIFFERENT group.
    {
        let mut g = c.benchmark_group("duckdb/list_unfiltered");
        for rows in LIST_SIZES {
            let data = dataset(READ_USERS, rows);
        let dir = tempfile::tempdir().unwrap();
        let conn = fresh_conn(&dir.path().join("bench.duckdb"));
        load(&conn, &data);
            let (name, clause) = ("unfiltered", "");
            let limit = LIST_CORE_LIMIT;
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={rows}/limit={limit}");

            // BDD-3: every engine's page must hold the same number of rows at the same
            // (rows, limit) point. A LIMIT that clamped differently would make the
            // cross-engine comparison meaningless while every number looked plausible.
            {
                let mut stmt = conn.prepare(&sql).unwrap();
                let n = duck_page(&mut stmt).len();
                assert_eq!(n, limit.min(rows), "BDD-3: duckdb page held {n} for limit={limit} over {rows}");
            }

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    std::hint::black_box(duck_page(&mut stmt).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    let page = duck_page(&mut stmt);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    // Limit sweep, unfiltered, at the core size.
    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        let dir = tempfile::tempdir().unwrap();
        let conn = fresh_conn(&dir.path().join("bench.duckdb"));
        load(&conn, &data);
        let mut g = c.benchmark_group("duckdb/list_unfiltered_limits");
        for limit in LIST_LIMITS {
            let rows = LIST_CORE_ROWS;
            let (name, clause) = ("unfiltered", "");
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={rows}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    std::hint::black_box(duck_page(&mut stmt).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    let page = duck_page(&mut stmt);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }
}

criterion_group!(benches, bench_insert, bench_bulk_load, bench_reads, bench_scan, bench_list);
criterion_main!(benches);
