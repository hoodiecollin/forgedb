use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use forgedb_benchmarks::{dataset, ts_from_seconds};
use uuid::Uuid;

macro_rules! mk_user {
    ($m:path, $row:expr, $i:expr) => {{
        use $m as _m;
        _m::User {
            id: Uuid::from_u128(0xF000_0000_0000_0000_0000_0000_0000_0000 + $i as u128),
            name: $row.name.clone(),
            email: format!("m{}@example.com", $i),
            created_at: ts_from_seconds($row.created_at),
            posts: (),
        }
    }};
}

macro_rules! bench_insert {
    ($group:expr, $m:path, $tag:literal, $pool:expr) => {{
        use $m as _m;
        let dir = tempfile::tempdir().unwrap();
        let mut db = _m::Database::open_at(dir.path().to_path_buf());
        let next = std::cell::Cell::new(0usize);
        $group.bench_function($tag, |b| {
            b.iter_batched(
                || {
                    let i = next.get();
                    next.set(i + 1);
                    mk_user!($m, &$pool.users[i % $pool.users.len()], i)
                },
                |u| {
                    db.user.insert(u).expect("insert");
                },
                BatchSize::SmallInput,
            );
        });
    }};
}

macro_rules! bench_update {
    ($group:expr, $m:path, $tag:literal, $pool:expr) => {{
        use $m as _m;
        let dir = tempfile::tempdir().unwrap();
        let mut db = _m::Database::open_at(dir.path().to_path_buf());
        let mut ids = Vec::new();
        for i in 0..64usize {
            let u = mk_user!($m, &$pool.users[i], i);
            ids.push(u.id);
            db.user.insert(u).expect("seed");
        }
        let next = std::cell::Cell::new(0usize);
        $group.bench_function($tag, |b| {
            b.iter_batched(
                || {
                    let i = next.get();
                    next.set(i + 1);
                    let id = ids[i % ids.len()];
                    let mut u = mk_user!($m, &$pool.users[i % $pool.users.len()], i);
                    u.id = id;
                    u.email = format!("u{}@example.com", i % ids.len());
                    (id, u)
                },
                |(id, u)| {
                    db.user.update(id, u).expect("update");
                },
                BatchSize::SmallInput,
            );
        });
    }};
}

macro_rules! bench_delete {
    ($group:expr, $m:path, $tag:literal, $pool:expr) => {{
        use $m as _m;
        let dir = tempfile::tempdir().unwrap();
        let mut db = _m::Database::open_at(dir.path().to_path_buf());
        for slot in 0..64usize {
            db.user.insert(mk_user!($m, &$pool.users[slot], slot)).expect("seed");
        }
        let next = std::cell::Cell::new(0usize);
        $group.bench_function($tag, |b| {
            b.iter(|| {
                let i = next.get();
                next.set(i + 1);
                let slot = i % 64;
                let u = mk_user!($m, &$pool.users[slot], slot);
                let id = u.id;
                db.user.delete(id);
                db.user.insert(u).expect("reinsert");
            });
        });
    }};
}

macro_rules! bench_churn {
    ($group:expr, $m:path, $tag:literal, $pool:expr, $n:expr) => {{
        use $m as _m;
        $group.bench_function($tag, |b| {
            b.iter_batched(
                || tempfile::tempdir().unwrap(),
                |dir| {
                    let mut db = _m::Database::open_at(dir.path().to_path_buf());
                    let mut u = mk_user!($m, &$pool.users[0], 0);
                    let id = u.id;
                    db.user.insert(u.clone()).expect("seed");
                    for i in 1..=$n {
                        u = mk_user!($m, &$pool.users[0], 0);
                        u.id = id;
                        u.email = "churn@example.com".to_string();
                        u.name = format!("v{i}");
                        db.user.update(id, u).expect("update");
                    }
                    db.maintain();
                    dir
                },
                BatchSize::PerIteration,
            );
        });
    }};
}

macro_rules! bench_reopen {
    ($group:expr, $m:path, $tag:literal, $pool:expr, $n:expr) => {{
        use $m as _m;
        let dir = tempfile::tempdir().unwrap();
        {
            let mut db = _m::Database::open_at(dir.path().to_path_buf());
            for i in 0..$n {
                db.user.insert(mk_user!($m, &$pool.users[i % $pool.users.len()], i)).expect("seed");
            }
            db.checkpoint();
        }
        $group.bench_function($tag, |b| {
            b.iter(|| {
                let db = _m::Database::open_at(dir.path().to_path_buf());
                std::hint::black_box(db.user.row_count());
            });
        });
    }};
}

fn bench_matrix_insert(c: &mut Criterion) {
    let pool = dataset(200_000, 0);
    let mut g = c.benchmark_group("matrix/insert_one");
    g.throughput(Throughput::Elements(1));
    bench_insert!(g, forgedb_benchmarks::v_default, "default", pool);
    bench_insert!(g, forgedb_benchmarks::v_fsync_never, "fsync_never", pool);
    bench_insert!(g, forgedb_benchmarks::v_replication_on, "replication_on", pool);
    bench_insert!(g, forgedb_benchmarks::v_changefeed_small, "changefeed_small", pool);
    g.finish();
}

fn bench_matrix_update(c: &mut Criterion) {
    let pool = dataset(200_000, 0);
    let mut g = c.benchmark_group("matrix/update_one");
    g.throughput(Throughput::Elements(1));
    bench_update!(g, forgedb_benchmarks::v_default, "default", pool);
    bench_update!(g, forgedb_benchmarks::v_fsync_never, "fsync_never", pool);
    bench_update!(g, forgedb_benchmarks::v_replication_on, "replication_on", pool);
    g.finish();
}

fn bench_matrix_delete(c: &mut Criterion) {
    let pool = dataset(200_000, 0);
    let mut g = c.benchmark_group("matrix/delete_reinsert");
    g.throughput(Throughput::Elements(1));
    bench_delete!(g, forgedb_benchmarks::v_default, "default", pool);
    bench_delete!(g, forgedb_benchmarks::v_fsync_never, "fsync_never", pool);
    bench_delete!(g, forgedb_benchmarks::v_replication_on, "replication_on", pool);
    g.finish();
}

fn bench_matrix_churn(c: &mut Criterion) {
    let pool = dataset(200_000, 0);
    let mut g = c.benchmark_group("matrix/churn_250_updates");
    g.sample_size(20);
    bench_churn!(g, forgedb_benchmarks::v_default, "default", pool, 250usize);
    bench_churn!(g, forgedb_benchmarks::v_compaction_off, "compaction_off", pool, 250usize);
    bench_churn!(g, forgedb_benchmarks::v_compaction_low, "compaction_low", pool, 250usize);
    g.finish();
}

fn bench_matrix_reopen(c: &mut Criterion) {
    let pool = dataset(200_000, 0);
    let mut g = c.benchmark_group("matrix/reopen_2000_rows");
    g.sample_size(20);
    bench_reopen!(g, forgedb_benchmarks::v_default, "default", pool, 2_000usize);
    bench_reopen!(g, forgedb_benchmarks::v_compaction_off, "compaction_off", pool, 2_000usize);
    g.finish();
}

criterion_group!(
    matrix,
    bench_matrix_insert,
    bench_matrix_update,
    bench_matrix_delete,
    bench_matrix_churn,
    bench_matrix_reopen,
);
criterion_main!(matrix);
