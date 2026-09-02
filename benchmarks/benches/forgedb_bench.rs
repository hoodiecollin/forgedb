use std::cell::Cell;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use forgedb_benchmarks::{dataset, id_for, Dataset};
use forgedb_benchmarks::forgedb_generated::{Database, Post, Tag, User};
use forgedb_benchmarks::ts_from_seconds;
use uuid::Uuid;

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

fn user_of(row: &forgedb_benchmarks::UserRow) -> User {
    User {
        id: Uuid::from_bytes(row.id),
        name: row.name.clone(),
        email: row.email.clone(),
        created_at: ts_from_seconds(row.created_at),
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
        created_at: ts_from_seconds(row.created_at),
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

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("forgedb/insert_user");
    group.throughput(Throughput::Elements(1));
    let pool = dataset(200_000, 0);
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    let next = Cell::new(0usize);
    group.bench_function("insert_one", |b| {
        b.iter_batched(
            || {
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
                    dir
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn bench_bulk_load_grouped(c: &mut Criterion) {
    let mut group = c.benchmark_group("forgedb/bulk_load_grouped");
    group.sample_size(10);
    for &n in &[1_000usize, 10_000] {
        let data = dataset(n.min(2_000).max(1), n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &data, |b, data| {
            b.iter_batched(
                || tempfile::tempdir().unwrap(),
                |dir| {
                    let mut db = Database::open_at(dir.path().to_path_buf());
                    db.transaction(|tx| {
                        for u in &data.users {
                            tx.create_user(user_of(u))?;
                        }
                        Ok(())
                    })
                    .expect("group-commit users");
                    db.transaction(|tx| {
                        for p in &data.posts {
                            tx.create_post(post_of(p))?;
                        }
                        Ok(())
                    })
                    .expect("group-commit posts");
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
    let (db, _dir) = populated(&data);

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

fn bench_scan(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let (db, _dir) = populated(&data);

    c.benchmark_group("forgedb/scan_aggregate")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("sum_views_where_published", |b| {
            b.iter(|| {
                let mut count = 0u64;
                let mut sum = 0u128;
                for row in db.post.all_agg() {
                    if row.published {
                        count += 1;
                        sum += row.views as u128;
                    }
                }
                std::hint::black_box((count, sum))
            });
        });

    c.benchmark_group("forgedb/scan_sort_top10")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("top10_by_views", |b| {
            b.iter(|| {
                let page = db
                    .post
                    .find_by_views_range(Some(50_000), None, true, Some(10));
                std::hint::black_box(page)
            });
        });
}

criterion_group!(
    benches,
    bench_insert,
    bench_bulk_load,
    bench_bulk_load_grouped,
    bench_reads,
    bench_scan
);
criterion_main!(benches);
