//! ForgeDB generated-code benchmark suite. Drives the real generated `Database`
//! API (`benchmarks/gen/database.rs`) over the shared seeded corpus. Scenario
//! numbers match docs/BENCHMARKS.md and the SQLite suite so the groups line up.

use std::cell::Cell;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use forgedb_benchmarks::{dataset, id_for, Dataset};
use forgedb_benchmarks::forgedb_generated::{Database, Post, Tag, User};
use forgedb_types::Timestamp;
use uuid::Uuid;

// Fixed corpus for the read/traversal scenarios (built once, outside timing).
// Kept modest because ForgeDB's `FsyncPolicy::Always` makes setup fsync-bound
// (one fsync per insert); point/probe latency is O(1) so this is representative.
const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

fn user_of(row: &forgedb_benchmarks::UserRow) -> User {
    User {
        id: Uuid::from_bytes(row.id),
        name: row.name.clone(),
        email: row.email.clone(),
        created_at: Timestamp::from_seconds(row.created_at),
        posts: (),
    }
}

fn post_of(row: &forgedb_benchmarks::PostRow) -> Post {
    Post {
        id: Uuid::from_bytes(row.id),
        title: row.title.clone(),
        views: row.views,
        published: row.published,
        author: Uuid::from_bytes(row.author),
        created_at: Timestamp::from_seconds(row.created_at),
        tags: (),
    }
}

fn tag_of(row: &forgedb_benchmarks::TagRow) -> Tag {
    Tag {
        id: Uuid::from_bytes(row.id),
        name: row.name.clone(),
        posts: (),
    }
}

/// Open a fresh on-disk database under a unique temp dir and load `data` into it
/// (users, posts, tags, then the M2M links). Returns the db + its tempdir guard.
fn populated(data: &Dataset) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    for u in &data.users {
        db.user.insert(user_of(u)).expect("insert user");
    }
    for p in &data.posts {
        db.post.insert(post_of(p)).expect("insert post");
    }
    for t in &data.tags {
        db.tag.insert(tag_of(t)).expect("insert tag");
    }
    for &(p, t) in &data.links {
        db.link_post_tag(Uuid::from_bytes(data.posts[p].id), Uuid::from_bytes(data.tags[t].id));
    }
    (db, dir)
}

// --- Scenario 2: single-row insert latency -----------------------------------
fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("forgedb/insert_user");
    group.throughput(Throughput::Elements(1));
    // A large pool of unique records; each iteration consumes the next so no
    // unique-email collision and the durable write path (WAL fsync) is timed.
    let pool = dataset(200_000, 0);
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    let next = Cell::new(0usize);
    group.bench_function("insert_one", |b| {
        b.iter_batched(
            || {
                // Build a fully-unique record in setup — criterion fills a whole
                // batch of inputs BEFORE running the routine, so the uniqueness
                // must be baked into each input, not read from the counter later.
                let i = next.get();
                next.set(i + 1);
                let mut u = user_of(&pool.users[i % pool.users.len()]);
                u.id = Uuid::from_u128(0xF000_0000_0000_0000_0000_0000_0000_0000 + i as u128);
                u.email = format!("insert{i}@example.com");
                u
            },
            |u| {
                db.user.insert(u).expect("insert");
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

// --- Scenario 1: bulk load ---------------------------------------------------
fn bench_bulk_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("forgedb/bulk_load_posts");
    group.sample_size(10);
    for &n in &[1_000usize, 10_000] {
        let data = dataset(n.min(2_000).max(1), n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || tempfile::tempdir().unwrap(),
                |dir| {
                    let mut db = Database::open_at(dir.path().to_path_buf());
                    for u in &data.users {
                        db.user.insert(user_of(u)).unwrap();
                    }
                    for p in &data.posts {
                        db.post.insert(post_of(p)).unwrap();
                    }
                    db.checkpoint();
                    dir // keep dir alive until after timing
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

// --- Read / traversal scenarios (5, 6, 8, 10, 11) ----------------------------
fn bench_reads(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let (db, _dir) = populated(&data);

    // Scenario 5: point lookup by PK.
    c.benchmark_group("forgedb/point_lookup")
        .throughput(Throughput::Elements(1))
        .bench_function("get_post_by_id", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = Uuid::from_bytes(id_for(2, i % READ_POSTS));
                i += 1;
                std::hint::black_box(db.post.get(id))
            });
        });

    // Scenario 6: secondary-index probe (unique email, O(1)).
    c.benchmark_group("forgedb/index_probe")
        .throughput(Throughput::Elements(1))
        .bench_function("get_user_by_email", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let email = format!("user{}@example.com", i % READ_USERS);
                i += 1;
                std::hint::black_box(db.user.get_by_email(&email))
            });
        });

    // Scenario 8: FK-index probe / Scenario 10: reverse one-to-many.
    c.benchmark_group("forgedb/reverse_fk")
        .throughput(Throughput::Elements(1))
        .bench_function("user_posts", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = Uuid::from_bytes(id_for(1, i % READ_USERS));
                i += 1;
                std::hint::black_box(db.user_posts(id))
            });
        });

    // Scenario 11: many-to-many traversal (linear junction scan today).
    c.benchmark_group("forgedb/m2m")
        .throughput(Throughput::Elements(1))
        .bench_function("post_tags", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = Uuid::from_bytes(id_for(2, i % READ_POSTS));
                i += 1;
                std::hint::black_box(db.post_tags(id))
            });
        });
}

// --- Scenario 7: filtered scan + aggregate + top-N ---------------------------
// The columnar-scan path. ForgeDB's honest scan is the generated narrow
// `__scan_all()` (decodes only the filterable/sortable columns, not the full
// record — the #160 list path), then, for the top-N, materialize ONLY the page.
// This is the scenario a columnar analytical engine (DuckDB) is built to win.
fn bench_scan(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let (db, _dir) = populated(&data);

    // 7a: full scan + aggregate — COUNT + SUM(views) WHERE published.
    c.benchmark_group("forgedb/scan_aggregate")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("sum_views_where_published", |b| {
            b.iter(|| {
                let mut count = 0u64;
                let mut sum = 0u128;
                for row in db.post.__scan_all() {
                    if row.published {
                        count += 1;
                        sum += row.views as u128;
                    }
                }
                std::hint::black_box((count, sum))
            });
        });

    // 7b: filtered scan + sort + page — top-10 posts by views (>= threshold),
    // materializing only the 10-row page (mirrors the generated #160 list path:
    // narrow scan/filter/sort, then full-materialize only the returned page).
    c.benchmark_group("forgedb/scan_sort_top10")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("top10_by_views", |b| {
            b.iter(|| {
                let mut rows: Vec<_> = db
                    .post
                    .__scan_all()
                    .into_iter()
                    .filter(|r| r.views >= 50_000)
                    .collect();
                rows.sort_unstable_by(|a, b| b.views.cmp(&a.views));
                rows.truncate(10);
                let page: Vec<_> = rows.iter().map(|r| db.post.get(r.id)).collect();
                std::hint::black_box(page)
            });
        });
}

criterion_group!(benches, bench_insert, bench_bulk_load, bench_reads, bench_scan);
criterion_main!(benches);
