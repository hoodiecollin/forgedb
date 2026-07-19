//! PostgreSQL benchmark suite (`postgres` crate, localhost unix socket). Mirrors
//! the other suites' scenarios over the SAME seeded corpus. PostgreSQL is a full
//! client/server RDBMS: every op carries **parse + plan + IPC** cost the embedded
//! engines (ForgeDB / SQLite / redb / DuckDB) do NOT pay, so this anchors the
//! "what a real RDBMS costs" axis rather than being head-to-head. Even over a unix
//! socket (no network), the protocol round-trip is real and is the point.
//!
//! Connection: the suite reads `FORGEDB_BENCH_PG_URL` (a libpq DSN, e.g.
//! `host=/tmp/pgsock user=me dbname=bench`). If it is unset the suite is a no-op
//! (prints guidance) so `cargo bench` without a running server does not fail — use
//! `make bench-postgres`, which spins an ephemeral cluster from the devbox-provided
//! `postgresql` package (no binary download) and sets the env.
//!
//! Durability: run at BOTH `synchronous_commit=on` (WAL fsync per commit — the
//! durable tier) and `off` (relaxed, group-commit) — never mixed in one chart.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use forgedb_benchmarks::{dataset, id_for, Dataset};
use postgres::{Client, NoTls};

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

const SCHEMA: &str = r#"
DROP TABLE IF EXISTS post_tag_link, post, tag, "user";
CREATE TABLE "user" (id BYTEA PRIMARY KEY, name TEXT, email TEXT UNIQUE, created_at BIGINT);
CREATE TABLE post (id BYTEA PRIMARY KEY, title TEXT, views BIGINT, published BOOLEAN, author BYTEA, created_at BIGINT);
CREATE INDEX post_author_idx ON post(author);
CREATE TABLE tag (id BYTEA PRIMARY KEY, name TEXT);
CREATE TABLE post_tag_link (post_id BYTEA, tag_id BYTEA);
CREATE INDEX ptl_post_idx ON post_tag_link(post_id);
"#;

/// Connect using `FORGEDB_BENCH_PG_URL`, or `None` (suite skipped) if unset.
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

/// Load `data` in one transaction (setup — not timed).
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

// --- Scenario 2: single-row insert latency (autocommit → WAL fsync per commit)
fn bench_insert(c: &mut Criterion) {
    let Some(mut client) = connect() else {
        skip_notice();
        return;
    };
    schema(&mut client);
    let mut group = c.benchmark_group("postgres/insert_user");
    group.throughput(Throughput::Elements(1));
    // Both durability groups insert into the same `email UNIQUE` table, so id +
    // email are namespaced per group (`gi`) — otherwise the second group collides
    // with the first group's rows. n grows monotonically within a group (unique).
    for (gi, &(sync, label)) in [("on", "sync_on"), ("off", "sync_off")].iter().enumerate() {
        client.batch_execute(&format!("SET synchronous_commit = {sync};")).unwrap();
        let stmt = client
            .prepare("INSERT INTO \"user\" (id, name, email, created_at) VALUES ($1,$2,$3,$4)")
            .unwrap();
        let base = (gi as u128) << 96; // distinct id space per durability group
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

// --- Read / traversal scenarios (5, 6, 8/10, 11) -----------------------------
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

    // Scenario 5: point lookup by PK.
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

    // Scenario 6: secondary-index probe (unique email).
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

    // Scenario 8/10: FK-index probe -> reverse one-to-many.
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

    // Scenario 11: many-to-many traversal (indexed junction join).
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

criterion_group!(benches, bench_insert, bench_reads);
criterion_main!(benches);
