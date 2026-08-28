use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use forgedb_benchmarks::{
    dataset, id_for, list_sql, ts_from_seconds, uuid_of, Dataset, PostJson, LIST_CORE_LIMIT,
    LIST_CORE_ROWS, LIST_LIMITS, LIST_SHAPES, LIST_SIZES,
};
use postgres::{Client, NoTls};

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
DROP TABLE IF EXISTS post_tag_link, post, tag, "user";
CREATE TABLE "user" (id BYTEA PRIMARY KEY, name TEXT, email TEXT UNIQUE, created_at BIGINT);
CREATE TABLE post (id BYTEA PRIMARY KEY, title TEXT, views BIGINT, published BOOLEAN, author BYTEA, created_at BIGINT);
CREATE INDEX post_author_idx ON post(author);
CREATE INDEX post_views_idx ON post(views);
CREATE TABLE tag (id BYTEA PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE post_tag_link (post_id BYTEA, tag_id BYTEA);
CREATE INDEX ptl_post_idx ON post_tag_link(post_id);
"#;

fn connect() -> Option<Client> {
    let dsn = std::env::var("FORGEDB_BENCH_PG_URL").ok()?;
    Some(Client::connect(&dsn, NoTls).expect("connect postgres"))
}

fn skip_notice() {
    eprintln!(
        "pg_bench: FORGEDB_BENCH_PG_URL unset — skipping the PostgreSQL suite. \
         Run `make bench-postgres` (spins an ephemeral cluster via devbox)."
    );
}

fn schema(client: &mut Client) {
    client.batch_execute(SCHEMA).expect("apply schema");
}

fn load(client: &mut Client, data: &Dataset) {
    let mut tx = client.transaction().unwrap();
    for u in &data.users {
        tx.execute(
            "INSERT INTO \"user\" (id, name, email, created_at) VALUES ($1,$2,$3,$4)",
            &[&&u.id[..], &u.name, &u.email, &u.created_at],
        )
        .unwrap();
    }
    for p in &data.posts {
        tx.execute(
            "INSERT INTO post (id, title, views, published, author, created_at) VALUES ($1,$2,$3,$4,$5,$6)",
            &[&&p.id[..], &p.title, &(p.views as i64), &p.published, &&p.author[..], &p.created_at],
        )
        .unwrap();
    }
    for t in &data.tags {
        tx.execute("INSERT INTO tag (id, name) VALUES ($1,$2)", &[&&t.id[..], &t.name]).unwrap();
    }
    for &(p, t) in &data.links {
        tx.execute(
            "INSERT INTO post_tag_link (post_id, tag_id) VALUES ($1,$2)",
            &[&&data.posts[p].id[..], &&data.tags[t].id[..]],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

fn bench_insert(c: &mut Criterion) {
    let Some(mut client) = connect() else {
        skip_notice();
        return;
    };
    schema(&mut client);
    let mut group = c.benchmark_group("postgres/insert_user");
    group.throughput(Throughput::Elements(1));
    for (gi, &(sync, label)) in [("on", "sync_on"), ("off", "sync_off")].iter().enumerate() {
        client.batch_execute(&format!("SET synchronous_commit = {sync};")).unwrap();
        let stmt = client
            .prepare("INSERT INTO \"user\" (id, name, email, created_at) VALUES ($1,$2,$3,$4)")
            .unwrap();
        let base = (gi as u128) << 96;
        let mut i = 0usize;
        group.bench_function(label, |b| {
            b.iter_batched(
                || {
                    let n = i;
                    i += 1;
                    n
                },
                |n| {
                    let id = uuid::Uuid::from_u128(
                        0xF000_0000_0000_0000_0000_0000_0000_0000 + base + n as u128,
                    )
                    .into_bytes();
                    let email = format!("insert_{label}_{n}@example.com");
                    client.execute(&stmt, &[&&id[..], &"bulk", &email, &1_700_000_000i64]).unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_reads(c: &mut Criterion) {
    let Some(mut client) = connect() else {
        skip_notice();
        return;
    };
    schema(&mut client);
    let data = dataset(READ_USERS, READ_POSTS);
    load(&mut client, &data);

    let get_post = client
        .prepare("SELECT id, title, views, published, author, created_at FROM post WHERE id = $1")
        .unwrap();
    let get_user = client
        .prepare("SELECT id, name, email, created_at FROM \"user\" WHERE email = $1")
        .unwrap();
    let by_author = client
        .prepare("SELECT id, title, views, published, author, created_at FROM post WHERE author = $1")
        .unwrap();
    let post_tags = client
        .prepare(
            "SELECT tag.id, tag.name FROM tag \
             JOIN post_tag_link l ON l.tag_id = tag.id WHERE l.post_id = $1",
        )
        .unwrap();

    c.benchmark_group("postgres/point_lookup")
        .throughput(Throughput::Elements(1))
        .bench_function("get_post_by_id", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(2, i % READ_POSTS);
                i += 1;
                let rows = client.query(&get_post, &[&&id[..]]).unwrap();
                std::hint::black_box(rows.into_iter().next().map(|r| {
                    let _id: Vec<u8> = r.get(0);
                    let _t: String = r.get(1);
                    let _v: i64 = r.get(2);
                    let _p: bool = r.get(3);
                    let _a: Vec<u8> = r.get(4);
                    let _c: i64 = r.get(5);
                }))
            });
        });

    c.benchmark_group("postgres/index_probe")
        .throughput(Throughput::Elements(1))
        .bench_function("get_user_by_email", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let email = format!("user{}@example.com", i % READ_USERS);
                i += 1;
                let rows = client.query(&get_user, &[&email]).unwrap();
                std::hint::black_box(rows.into_iter().next().map(|r| {
                    let _id: Vec<u8> = r.get(0);
                    let _n: String = r.get(1);
                    let _e: String = r.get(2);
                    let _c: i64 = r.get(3);
                }))
            });
        });

    c.benchmark_group("postgres/reverse_fk")
        .throughput(Throughput::Elements(1))
        .bench_function("user_posts", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(1, i % READ_USERS);
                i += 1;
                let rows = client.query(&by_author, &[&&id[..]]).unwrap();
                std::hint::black_box(rows.len())
            });
        });

    c.benchmark_group("postgres/m2m")
        .throughput(Throughput::Elements(1))
        .bench_function("post_tags", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(2, i % READ_POSTS);
                i += 1;
                let rows = client.query(&post_tags, &[&&id[..]]).unwrap();
                std::hint::black_box(rows.len())
            });
        });
}

fn pg_page(client: &mut Client, sql: &str) -> Vec<PostJson> {
    client
        .query(sql, &[])
        .unwrap()
        .into_iter()
        .map(|r| PostJson {
            id: uuid_of(r.get::<_, &[u8]>(0).to_vec()),
            title: r.get(1),
            views: r.get::<_, i64>(2) as u64,
            published: r.get(3),
            author: uuid_of(r.get::<_, &[u8]>(4).to_vec()),
            created_at: ts_from_seconds(r.get(5)),
            tags: (),
        })
        .collect()
}

fn bench_list(c: &mut Criterion) {
    let Some(mut client) = connect() else {
        skip_notice();
        return;
    };
    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        schema(&mut client);
        load(&mut client, &data);
        let mut g = c.benchmark_group("pg/list_core");
        for (name, clause) in LIST_SHAPES {
            let limit = LIST_CORE_LIMIT;
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={LIST_CORE_ROWS}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                b.iter(|| {
                    std::hint::black_box(pg_page(&mut client, &sql).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                b.iter(|| {
                    let page = pg_page(&mut client, &sql);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    {
        let mut g = c.benchmark_group("pg/list_unfiltered");
        for rows in LIST_SIZES {
            let data = dataset(READ_USERS, rows);
        schema(&mut client);
        load(&mut client, &data);
            let (name, clause) = ("unfiltered", "");
            let limit = LIST_CORE_LIMIT;
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={rows}/limit={limit}");

            {

                let n = pg_page(&mut client, &sql).len();
                assert_eq!(n, limit.min(rows), "BDD-3: pg page held {n} for limit={limit} over {rows}");
            }

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                b.iter(|| {
                    std::hint::black_box(pg_page(&mut client, &sql).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                b.iter(|| {
                    let page = pg_page(&mut client, &sql);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        schema(&mut client);
        load(&mut client, &data);
        let mut g = c.benchmark_group("pg/list_unfiltered_limits");
        for limit in LIST_LIMITS {
            let rows = LIST_CORE_ROWS;
            let (name, clause) = ("unfiltered", "");
            let sql = list_sql(clause, limit, 0);
            let label = format!("{name}/rows={rows}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                b.iter(|| {
                    std::hint::black_box(pg_page(&mut client, &sql).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                b.iter(|| {
                    let page = pg_page(&mut client, &sql);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }
}

criterion_group!(benches, bench_insert, bench_reads, bench_list);
criterion_main!(benches);
