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

fn bench_reads(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let conn = fresh_conn(&dir.path().join("bench.duckdb"));
    load(&conn, &data);

    type PostRowOut = (Vec<u8>, String, u64, bool, Vec<u8>, i64);
    type UserRowOut = (Vec<u8>, String, String, i64);
    type TagRowOut = (Vec<u8>, String);

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

fn bench_scan(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let conn = fresh_conn(&dir.path().join("bench.duckdb"));
    load(&conn, &data);

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
