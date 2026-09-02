use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use forgedb_benchmarks::{
    dataset, id_for, list_sql, ts_from_seconds, uuid_of, Dataset, PostJson, LIST_CORE_LIMIT,
    LIST_CORE_ROWS, LIST_LIMITS, LIST_SHAPES, LIST_SIZES,
};
use rusqlite::{params, Connection};

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

const SCHEMA: &str = include_str!("../schema.sql");

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

fn fresh_conn(path: &std::path::Path) -> Connection {
    fresh_conn_dur(path, false)
}

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

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("sqlite/insert_user");
    group.throughput(Throughput::Elements(1));
    for &fullfsync in &[false, true] {
        let dir = tempfile::tempdir().unwrap();
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

fn bench_reads(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let mut conn = fresh_conn(&dir.path().join("bench.db"));
    load(&mut conn, &data);

    type PostRowOut = (Vec<u8>, String, i64, i64, Vec<u8>, i64);
    type UserRowOut = (Vec<u8>, String, String, i64);
    type TagRowOut = (Vec<u8>, String);

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

fn bench_scan(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let mut conn = fresh_conn(&dir.path().join("bench.db"));
    load(&mut conn, &data);

    c.benchmark_group("sqlite/scan_aggregate")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("sum_views_where_published", |b| {
            let mut stmt = conn
                .prepare("SELECT COUNT(*), COALESCE(SUM(views), 0) FROM post WHERE published = 1")
                .unwrap();
            b.iter(|| {
                let row: (i64, i64) = stmt.query_row([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
                std::hint::black_box(row)
            });
        });

    type PostRowOut = (Vec<u8>, String, i64, i64, Vec<u8>, i64);
    c.benchmark_group("sqlite/scan_sort_top10")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("top10_by_views", |b| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, views, published, author, created_at FROM post \
                     WHERE views >= ?1 ORDER BY views DESC LIMIT 10",
                )
                .unwrap();
            b.iter(|| {
                let rows: Vec<PostRowOut> = stmt
                    .query_map(params![50_000i64], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                    })
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                std::hint::black_box(rows)
            });
        });
}

fn sqlite_page(stmt: &mut rusqlite::Statement<'_>) -> Vec<PostJson> {
    stmt.query_map([], |r| {
        Ok(PostJson {
            id: uuid_of(r.get(0)?),
            title: r.get(1)?,
            views: r.get::<_, i64>(2)? as u64,
            published: r.get::<_, i64>(3)? != 0,
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
        let mut conn = fresh_conn(&dir.path().join("bench.db"));
        load(&mut conn, &data);
        let mut g = c.benchmark_group("sqlite/list_core");
        for (name, clause) in LIST_SHAPES {
            let limit = LIST_CORE_LIMIT;
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={LIST_CORE_ROWS}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    std::hint::black_box(sqlite_page(&mut stmt).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    let page = sqlite_page(&mut stmt);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    {
        let mut g = c.benchmark_group("sqlite/list_unfiltered");
        for rows in LIST_SIZES {
            let data = dataset(READ_USERS, rows);
        let dir = tempfile::tempdir().unwrap();
        let mut conn = fresh_conn(&dir.path().join("bench.db"));
        load(&mut conn, &data);
            let (name, clause) = ("unfiltered", "");
            let limit = LIST_CORE_LIMIT;
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={rows}/limit={limit}");

            {
                let mut stmt = conn.prepare(&sql).unwrap();
                let n = sqlite_page(&mut stmt).len();
                assert_eq!(n, limit.min(rows), "BDD-3: sqlite page held {n} for limit={limit} over {rows}");
            }

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    std::hint::black_box(sqlite_page(&mut stmt).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    let page = sqlite_page(&mut stmt);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        let dir = tempfile::tempdir().unwrap();
        let mut conn = fresh_conn(&dir.path().join("bench.db"));
        load(&mut conn, &data);
        let mut g = c.benchmark_group("sqlite/list_unfiltered_limits");
        for limit in LIST_LIMITS {
            let rows = LIST_CORE_ROWS;
            let (name, clause) = ("unfiltered", "");
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={rows}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    std::hint::black_box(sqlite_page(&mut stmt).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                let mut stmt = conn.prepare(&sql).unwrap();
                b.iter(|| {
                    let page = sqlite_page(&mut stmt);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }
}

criterion_group!(benches, bench_insert, bench_bulk_load, bench_reads, bench_scan, bench_list);
criterion_main!(benches);
