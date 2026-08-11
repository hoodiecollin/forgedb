//! Concurrency report (scenario 16): ForgeDB reader throughput under a live writer.
//!
//! #56-B (single writer + concurrent readers) claims reads are LOCK-FREE under a
//! live single writer — a `DatabaseReader` captured from the writer reads its own
//! consistent snapshot with no lock shared with the write path. This measures
//! aggregate reader throughput at 1/2/4/8 reader threads, each WITH and WITHOUT a
//! background writer hammering inserts, so the claim is falsifiable: if reads were
//! serialized against the writer, the "with writer" column would collapse.
//!
//! Not a Criterion micro-timing (it measures sustained throughput under background
//! load), so it lives as an example. Run:
//!   cargo run --manifest-path benchmarks/Cargo.toml --example concurrency --release

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use forgedb_benchmarks::forgedb_generated::{Database, User};
use forgedb_benchmarks::{dataset, id_for};
use forgedb_benchmarks::ts_from_seconds;
use uuid::Uuid;

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;
const DURATION: Duration = Duration::from_secs(2);
const WRITE_BASE: u128 = 0xF000_0000_0000_0000_0000_0000_0000_0000;

/// Aggregate reads/sec across `threads` reader threads for `DURATION`, and (if
/// `with_writer`) writes/sec of one background writer running concurrently.
fn run(threads: usize, with_writer: bool) -> (f64, f64) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    for u in &data.users {
        db.user
            .insert(User {
                id: Uuid::from_bytes(u.id),
                name: u.name.clone(),
                email: u.email.clone(),
                created_at: ts_from_seconds(u.created_at),
                posts: (),
            })
            .unwrap();
    }
    for p in &data.posts {
        db.post
            .insert(forgedb_benchmarks::forgedb_generated::Post {
                id: Uuid::from_bytes(p.id),
                title: p.title.clone(),
                views: p.views,
                published: p.published,
                author: Uuid::from_bytes(p.author),
                created_at: ts_from_seconds(p.created_at),
                tags: (),
            })
            .unwrap();
    }

    // Capture a consistent reader + snapshot BEFORE moving the db to the writer.
    // Both are owned ('static): reader handles are try_clone'd fds + Arc'd index
    // maps, the snapshot is a plain row-count watermark.
    let snap = db.snapshot();
    let reader = Arc::new((db.reader(), snap));

    let stop = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicU64::new(0));
    let writes = Arc::new(AtomicU64::new(0));

    // Keep the db alive: move it into the writer thread, OR hold it here so its
    // files (which the reader's cloned fds point at) stay open for read-only runs.
    let _hold;
    let writer = if with_writer {
        let stop = stop.clone();
        let writes = writes.clone();
        _hold = None::<Database>;
        Some(thread::spawn(move || {
            let mut n: u128 = 0;
            while !stop.load(Ordering::Relaxed) {
                let id = Uuid::from_u128(WRITE_BASE + n);
                let _ = db.user.insert(User {
                    id,
                    name: "w".into(),
                    email: format!("conc{n}@example.com"),
                    created_at: ts_from_seconds(1_700_000_000),
                    posts: (),
                });
                writes.fetch_add(1, Ordering::Relaxed);
                n += 1;
            }
        }))
    } else {
        _hold = Some(db);
        None
    };

    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let reader = reader.clone();
            let stop = stop.clone();
            let reads = reads.clone();
            thread::spawn(move || {
                let mut i = t;
                let mut local = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    let id = Uuid::from_bytes(id_for(2, i % READ_POSTS));
                    std::hint::black_box(reader.0.post.get_at(&reader.1.post, id));
                    i += 1;
                    local += 1;
                }
                reads.fetch_add(local, Ordering::Relaxed);
            })
        })
        .collect();

    thread::sleep(DURATION);
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        h.join().unwrap();
    }
    if let Some(w) = writer {
        w.join().unwrap();
    }
    let secs = start.elapsed().as_secs_f64();
    (reads.load(Ordering::Relaxed) as f64 / secs, writes.load(Ordering::Relaxed) as f64 / secs)
}

fn main() {
    println!(
        "ForgeDB concurrent reads under a live writer (#56-B), {} s per cell, \
         {READ_POSTS} posts loaded.\n",
        DURATION.as_secs()
    );
    println!("  {:<10} {:>16} {:>16} {:>16}", "readers", "reads/s (idle)", "reads/s (+writer)", "writes/s");
    for &threads in &[1usize, 2, 4, 8] {
        let (idle, _) = run(threads, false);
        let (busy, w) = run(threads, true);
        println!(
            "  {:<10} {:>16.0} {:>16.0} {:>16.0}",
            threads, idle, busy, w
        );
    }
    println!(
        "\nIf reads were serialized against the writer, the (+writer) column would \
         collapse toward the writer's rate; #56-B predicts it stays near the idle column."
    );
}
