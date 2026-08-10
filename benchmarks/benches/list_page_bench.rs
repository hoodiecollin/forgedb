//! #226 kill gate — how much of a list request is the per-row page materialization?
//!
//! # What this decides
//!
//! #226 proposes serializing the list page from the buffers `__with_scan` already
//! holds, instead of re-reading each page row through `get(id)`. Its own body sets a
//! kill gate: *if the delta is inside the noise floor at realistic page sizes, close
//! the issue rather than build it.*
//!
//! Measuring the delta directly would require prototyping the buffered page decode —
//! i.e. building the thing the gate exists to decide. So this measures the **ceiling**
//! instead. The generated handler (`crates/codegen/src/api.rs`, `live_list_block`) is
//! exactly three phases:
//!
//! ```text
//!   A  __with_scan(sel, keep, ..)  -> (total, page_ids)     // filter + sort + paginate
//!   B  page_ids.filter_map(get)    -> Vec<Model>            // <-- all #226 can remove
//!   C  serde_json                  -> the response body
//! ```
//!
//! #226 replaces B with a decode from A's buffers. It cannot make B free (a gather
//! still costs something) and it does not touch A or C. So **B / (A+B+C) is a hard
//! upper bound on the win**, and it needs no prototype to measure. If the ceiling is
//! inside the noise floor, the kill is decisive; if it is large, the issue survives
//! and Gate 2 proceeds with a number attached.
//!
//! # Why `Doc`, and why unfiltered
//!
//! `Doc` is the most #226-favourable model in `bench.forge`: four `string` fields, so
//! phase B pays four `String` allocations per page row — the largest per-row cost any
//! bench model offers. Biasing the subject *toward* the feature is the right call for
//! a gate that can only kill it.
//!
//! The request is unfiltered because that is #224's stated blind spot ("the unfiltered
//! ones where #224 wins nothing"), and because `sel = None` is what an ordinary
//! `GET /api/doc?limit=50` sends.
//!
//! # Reading the output
//!
//! Compare `full_path` against `page_get` at the same (rows, limit). `page_get /
//! full_path` is the ceiling. `scan_only` and `serialize` are reported so the
//! remainder is attributable rather than inferred.
//!
//! ```bash
//! make list-page-bench
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use forgedb_benchmarks::forgedb_generated::{Database, Doc, DocScanRef};
use uuid::Uuid;

/// Per-field body length. 200 chars is a realistic description/body field and keeps
/// the four columns' bytes well above the noise of the fixed columns beside them.
const BODY_LEN: usize = 200;

/// Table sizes. 1k and 10k rather than one size, because the ceiling is a *ratio*:
/// phase A is O(live rows) and phase B is O(limit), so the fraction necessarily moves
/// with table size. A single measurement would not reveal that, and a kill (or a
/// survival) argued from one table size would be arguing from a coincidence.
const ROWS: [usize; 2] = [1_000, 10_000];

/// `DEFAULT_LIMIT` and `MAX_LIMIT` from `forgedb-query-params` — the two page sizes
/// #226's gate names. (Not imported: the detached bench project deliberately does not
/// depend on the substrate crate the generated `api.rs` links.)
const LIMITS: [usize; 2] = [50, 1_000];

fn doc_of(i: usize) -> Doc {
    // Deterministic ids so a rerun measures the same rows in the same order.
    let mut bytes = [0u8; 16];
    bytes[0] = 9;
    bytes[8..16].copy_from_slice(&(i as u64).to_be_bytes());
    let body = |tag: char| -> String {
        let mut s = String::with_capacity(BODY_LEN);
        s.push(tag);
        while s.len() < BODY_LEN {
            s.push((b'a' + ((i + s.len()) % 26) as u8) as char);
        }
        s
    };
    Doc {
        id: Uuid::from_bytes(bytes),
        seq: i as u64,
        kind: (i % 7) as u32,
        body_a: body('a'),
        body_b: body('b'),
        body_c: body('c'),
        body_d: body('d'),
    }
}

fn populated(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    for i in 0..n {
        db.doc.insert(doc_of(i)).expect("insert doc");
    }
    (db, dir)
}

/// Phase A, verbatim from the generated handler's shape: filter + sort + paginate
/// inside the scan scope, with only `(total, ids)` crossing the closure boundary.
/// `keep` is `|_| true` and the sort is a no-op, which is what an unfiltered,
/// unsorted list request generates.
fn phase_a(db: &Database, limit: usize) -> (usize, Vec<Uuid>) {
    db.doc.__with_scan(
        None,
        |_: &DocScanRef<'_>| true,
        |scan: &mut Vec<DocScanRef<'_>>| {
            let total = scan.len();
            // `Pagination::apply` at offset 0 is `&items[0..end.min(len)]`.
            let end = limit.min(scan.len());
            let ids: Vec<Uuid> = scan[0..end].iter().map(|r| r.id).collect();
            (total, ids)
        },
    )
}

/// Phase B — the only phase #226 can remove.
fn phase_b(db: &Database, ids: &[Uuid]) -> Vec<Doc> {
    ids.iter().filter_map(|id| db.doc.get(*id)).collect()
}

fn bench_list_page(c: &mut Criterion) {
    for rows in ROWS {
        let (db, _dir) = populated(rows);

        for limit in LIMITS {
            let label = format!("rows={rows}/limit={limit}");

            // Precomputed inputs for the isolated phases, so B is not billed for A.
            let (_, ids) = phase_a(&db, limit);
            let page = phase_b(&db, &ids);

            let mut g = c.benchmark_group("forgedb/list_page");

            g.bench_with_input(
                BenchmarkId::new("full_path", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| {
                        let (total, ids) = phase_a(&db, limit);
                        let page = phase_b(&db, &ids);
                        let body = serde_json::to_string(&page).expect("serialize");
                        std::hint::black_box((total, body))
                    });
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(phase_a(&db, limit)));
            });

            g.bench_with_input(BenchmarkId::new("page_get", &label), &ids, |b, ids| {
                b.iter(|| std::hint::black_box(phase_b(&db, ids)));
            });

            g.bench_with_input(BenchmarkId::new("serialize", &label), &page, |b, page| {
                b.iter(|| std::hint::black_box(serde_json::to_string(page).expect("serialize")));
            });

            g.finish();
        }
    }
}

criterion_group!(benches, bench_list_page);
criterion_main!(benches);
