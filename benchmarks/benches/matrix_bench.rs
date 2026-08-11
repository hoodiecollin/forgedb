//! Config-matrix benchmark suite (epic #126): the SAME scenarios run against the
//! SAME `bench.forge` generated under several `forgedb.toml` configs, so Criterion
//! reports how each generate-time knob shifts performance. Each config is a
//! distinct generated module (`forgedb_benchmarks::variants::*`); a scenario is a
//! macro invoked once per relevant variant into a shared Criterion group, so the
//! configs line up side by side in the report.
//!
//! Scenario → variants mapping (only the config axes a scenario is sensitive to):
//!   write-path latency (insert/update/delete): default, fsync_never, replication_on
//!   changefeed capacity:                       insert_one adds changefeed_small
//!   compaction churn + maintain():             default, compaction_off, compaction_low
//!   cold-start reopen:                         default, compaction_off
//!
//! Regenerate the variant modules with `make bench-regen-matrix` after any codegen
//! change (each module compiling is itself a codegen guard).

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use forgedb_benchmarks::{dataset, ts_from_seconds};
use uuid::Uuid;

// A unique user record for a write-path iteration: id + email are made unique by
// `i` so inserts never collide on the `&unique` email and the durable write path
// (WAL fsync / broker) is what's timed. `$m` is a variant module path.
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

// insert_one: fresh unique record per iteration into a shared on-disk db.
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

// update_one: repeatedly supersede a small live set (one dead version per update).
macro_rules! bench_update {
    ($group:expr, $m:path, $tag:literal, $pool:expr) => {{
        use $m as _m;
        let dir = tempfile::tempdir().unwrap();
        let mut db = _m::Database::open_at(dir.path().to_path_buf());
        // Seed a small live set to update in place.
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
                    u.id = id; // update the existing id (supersede)
                    u.email = format!("u{}@example.com", i % ids.len()); // stable per id
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

// delete_reinsert: each iteration deletes a slot's record (tombstone append +
// fsync) then re-inserts the SAME id + email (delete cleared the unique-index
// entry, so the reinsert is valid) — the live set stays stable and every
// iteration times one durable delete + one durable insert. A pair-cost proxy for
// delete on the fsync-bound write path (pure delete cannot be isolated without an
// untimed reinsert per iteration, which criterion's batched setup cannot stage
// per-op against a shared &mut db).
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
                // Stable id + email for this slot (mk_user derives both from `slot`).
                let u = mk_user!($m, &$pool.users[slot], slot);
                let id = u.id;
                db.user.delete(id);
                db.user.insert(u).expect("reinsert");
            });
        });
    }};
}

// churn: N updates to ONE id — crosses the compaction threshold on the low/default
// configs (triggering the deferred flag / inline ceiling) and never on
// compaction_off. Times the whole burst so compaction cost (if any) is included.
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
                    db.maintain(); // #162-A: run any deferred compaction off the write turn
                    dir
                },
                BatchSize::PerIteration,
            );
        });
    }};
}

// reopen: seed a dir with `n` users (outside timing), then time Database::open_at
// (the rehydrate — id_to_row + secondary index rebuild) on that populated dir.
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
    // 250 updates crosses the low threshold (100) — inline ceiling / maintain
    // reclaim — and the default (1000) sets compaction_due but maintain() runs it;
    // compaction_off never reclaims (grows to 251 physical rows).
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
