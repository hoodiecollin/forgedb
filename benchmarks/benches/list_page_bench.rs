//! #226 — how much of a list request is the per-row page materialization, and how
//! much of that did the buffered page decode actually recover?
//!
//! # Two questions, one file, on purpose
//!
//! This bench was written for #226's **kill gate**: *if the delta is inside the noise
//! floor at realistic page sizes, close the issue rather than build it.* At that point
//! the buffered decode did not exist, so measuring the delta directly would have meant
//! prototyping the very thing the gate existed to authorize. It measured the
//! **ceiling** instead. The pre-#226 handler was exactly three phases:
//!
//! ```text
//!   A  __with_scan(sel, keep, ..)  -> (total, page_ids)     // filter + sort + paginate
//!   B  page_ids.filter_map(get)    -> Vec<Model>            // <-- all #226 can remove
//!   C  serde_json                  -> the response body
//! ```
//!
//! #226 could remove no more than B, so `B / (A+B+C)` bounded the win with no
//! prototype needed. The gate did not fire (21.8% at the worst measured point against
//! a ±2.4% band).
//!
//! The decode now exists — `__with_page` + `<Model>PageRef` (`crates/codegen/src/rust.rs`,
//! wired at `crates/codegen/src/api.rs`'s `page_scope_return`). So this file now answers
//! the second question too: **realized vs ceiling.** The post-#226 path is
//!
//! ```text
//!   A   the same scan, filter, sort and paginate, inside __with_page
//!   B'  gather the page's remaining columns + build one PageRef per page row
//!   C'  serde_json over &[PageRef] instead of over Vec<Model>
//! ```
//!
//! # Why both paths are measured in ONE binary
//!
//! `__with_scan` and `get` both still exist at HEAD (the live-query sites and the
//! `?projection=` arms still need owned rows), so the **pre-#226 path is reproducible
//! in the same binary, over the same data, in the same Criterion run** as the new one.
//! That is a paired A/B with an in-run control, not a cross-commit comparison against a
//! table measured somewhere else — which this project has a standing rule against, and
//! which is exactly how an unfair "slower/faster" claim gets published.
//!
//! `full_path` and `page_get` therefore double as the baseline, re-measured here rather
//! than quoted. One honest caveat: #226 added `__slot` to `<Model>ScanRef`, so phase A
//! costs one extra `usize` store per scanned row in *both* arms. It inflates the control
//! and the treatment equally, and `scan_only` is reported so the size of that store can
//! be checked against the pre-#226 A.
//!
//! # Why `Doc`, and why also `Post`
//!
//! `Doc` is the most #226-favourable model in `bench.forge`: four `string` fields, so
//! phase B pays four `String` allocations per page row — the largest per-row cost any
//! bench model offers. Biasing the subject *toward* the feature was the right call for a
//! gate that could only kill it, and keeping the same subject is what makes realized
//! comparable to ceiling.
//!
//! But `Doc` is also #226's **best** case on a second axis that only matters now that
//! the win is real: every `Doc` field is filterable, so `DocPageRef`'s field set equals
//! `DocScanRef`'s and **B' gathers no columns at all** — it only copies the scan view.
//! Reporting that alone would present the best case as if it were general. `Post` is the
//! in-run control for exactly that: `PostScanRef` misses `author: Uuid`
//! (`_ => false` in `is_filterable_field` for an FK), so `Post`'s B' really does run a
//! second gather, over one fixed-width column, and it has one `string` field rather than
//! four. If the win survives on `Post` it is not an artifact of `Doc`'s empty gather.
//!
//! Both are unfiltered because that is #224's stated blind spot ("the unfiltered ones
//! where #224 wins nothing"), and because `sel = None` is what an ordinary
//! `GET /api/doc?limit=50` sends.
//!
//! # Reading the output
//!
//! Per `(rows, limit)`, six arms. Four are the pre-#226 path, two are the post-#226 one:
//!
//! | arm | phases | what it is |
//! |---|---|---|
//! | `full_path` | A+B+C | pre-#226 request, the in-run control |
//! | `full_buffered` | A+B'+C' | post-#226 request |
//! | `scan_only` | A | shared by both paths |
//! | `page_buffered` | A+B' | the new scope, views forced live, not serialized |
//! | `page_get` | B | what #226 removes — **the ceiling** |
//! | `serialize` | C | serde over `Vec<Model>` |
//!
//! They are registered in that order deliberately: Criterion measures arms in
//! registration order, so each compared pair (`full_path`/`full_buffered`,
//! `scan_only`/`page_buffered`) runs back to back rather than minutes apart.
//! Reordering them weakens the pairing.
//!
//! - **realized win** = `full_path − full_buffered`; **ceiling** = `page_get`. Realized
//!   must be ≤ ceiling: #226 touches neither A nor C, so a realized win exceeding B
//!   means the two arms are not doing the same work.
//! - **B'** = `page_buffered − scan_only`, and **C'** = `full_buffered − page_buffered`.
//!   `C' ≈ serialize` is the consistency check on that subtraction, and it is also what
//!   bounds `page_buffered`'s black-box fold (below) as negligible.
//! - `full_path` excludes axum/HTTP overhead, which **inflates** B's share of the
//!   request. The bias runs in #226's favour, so quote the absolute saving and the
//!   per-row cost, not just the percentage.
//!
//! `page_buffered` folds a cheap checksum over every field of every page view before
//! returning. Without it the view construction is dead code inside the scope and LLVM
//! may delete it, which would report B' as ~0. The fold reads `&str` lengths and
//! integers — no allocation, no copy — and `C' ≈ serialize` is the evidence that what it
//! adds is below the noise floor.
//!
//! ```bash
//! make bench-list-page
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use forgedb_benchmarks::forgedb_generated::{
    Database, Doc, DocPageRef, DocScanRef, Post, PostPageRef, PostScanRef, User,
};
use forgedb_types::Timestamp;
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

/// Deterministic ids so a rerun measures the same rows in the same order. `tag`
/// separates the models' id spaces so nothing can alias across subjects.
fn id_of(tag: u8, i: usize) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = tag;
    bytes[8..16].copy_from_slice(&(i as u64).to_be_bytes());
    Uuid::from_bytes(bytes)
}

fn body_of(i: usize, tag: char) -> String {
    let mut s = String::with_capacity(BODY_LEN);
    s.push(tag);
    while s.len() < BODY_LEN {
        s.push((b'a' + ((i + s.len()) % 26) as u8) as char);
    }
    s
}

fn doc_of(i: usize) -> Doc {
    Doc {
        id: id_of(9, i),
        seq: i as u64,
        kind: (i % 7) as u32,
        body_a: body_of(i, 'a'),
        body_b: body_of(i, 'b'),
        body_c: body_of(i, 'c'),
        body_d: body_of(i, 'd'),
    }
}

fn populated_docs(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    for i in 0..n {
        db.doc.insert(doc_of(i)).expect("insert doc");
    }
    (db, dir)
}

/// One author for every post: the FK is required, so *some* `User` row has to exist,
/// but how many there are is irrelevant to a `Post` list read (the page never
/// traverses the relation).
const AUTHOR: u8 = 7;

/// Base instant for `created_at`, in MICROSECONDS — `Timestamp`'s only unit since
/// #254, which removed `Timestamp::from_seconds` outright rather than deprecating it.
/// The sibling benches on this branch still call it and so do not compile; #279 fixed
/// that on `develop` by adding a shared `forgedb_benchmarks::ts_from_seconds` helper.
/// **Post-merge, switch this to that helper** — one seconds→micros conversion for the
/// whole bench project is the point of it, and a second spelling here defeats it.
const BASE_US: i64 = 1_700_000_000_000_000;

fn populated_posts(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    let author = id_of(AUTHOR, 0);
    db.user
        .insert(User {
            id: author,
            name: "bench".to_string(),
            email: "bench@example.com".to_string(),
            created_at: Timestamp::from_micros(BASE_US),
            posts: (),
        })
        .expect("insert user");
    for i in 0..n {
        db.post
            .insert(Post {
                id: id_of(8, i),
                title: body_of(i, 't'),
                views: i as u64,
                published: i % 2 == 0,
                author,
                created_at: Timestamp::from_micros(BASE_US + i as i64 * 1_000_000),
                tags: (),
            })
            .expect("insert post");
    }
    (db, dir)
}

/// Phase A, verbatim from the pre-#226 handler's shape: filter + sort + paginate
/// inside the scan scope, with only `(total, ids)` crossing the closure boundary.
/// `keep` is `|_| true` and the sort is a no-op, which is what an unfiltered,
/// unsorted list request generates.
fn doc_phase_a(db: &Database, limit: usize) -> (usize, Vec<Uuid>) {
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

/// Phase B — the only phase #226 removes.
fn doc_phase_b(db: &Database, ids: &[Uuid]) -> Vec<Doc> {
    ids.iter().filter_map(|id| db.doc.get(*id)).collect()
}

/// Phases A+B', post-#226: the same scan/filter/sort/paginate, then the page gather
/// and one `DocPageRef` per page row. The fold is what keeps the view construction
/// from being dead code — see the module docs.
fn doc_page_buffered(db: &Database, limit: usize) -> (usize, usize) {
    db.doc.__with_page(
        None,
        |_: &DocScanRef<'_>| true,
        |_: &mut Vec<DocScanRef<'_>>| {},
        0,
        limit,
        |total: usize, page: &[DocPageRef<'_>]| {
            let mut sum = 0usize;
            for r in page {
                sum ^= r.id.as_u128() as usize;
                sum ^= r.seq as usize;
                sum ^= r.kind as usize;
                sum ^= r.body_a.len();
                sum ^= r.body_b.len();
                sum ^= r.body_c.len();
                sum ^= r.body_d.len();
            }
            (total, sum)
        },
    )
}

/// Phases A+B'+C' — the whole post-#226 request path, exactly as the generated
/// handler runs it: everything inside the scope, and only an owned value escapes.
fn doc_full_buffered(db: &Database, limit: usize) -> (usize, String) {
    db.doc.__with_page(
        None,
        |_: &DocScanRef<'_>| true,
        |_: &mut Vec<DocScanRef<'_>>| {},
        0,
        limit,
        |total: usize, page: &[DocPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

fn post_phase_a(db: &Database, limit: usize) -> (usize, Vec<Uuid>) {
    db.post.__with_scan(
        None,
        |_: &PostScanRef<'_>| true,
        |scan: &mut Vec<PostScanRef<'_>>| {
            let total = scan.len();
            let end = limit.min(scan.len());
            let ids: Vec<Uuid> = scan[0..end].iter().map(|r| r.id).collect();
            (total, ids)
        },
    )
}

fn post_phase_b(db: &Database, ids: &[Uuid]) -> Vec<Post> {
    ids.iter().filter_map(|id| db.post.get(*id)).collect()
}

fn post_page_buffered(db: &Database, limit: usize) -> (usize, usize) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        0,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            let mut sum = 0usize;
            for r in page {
                sum ^= r.id.as_u128() as usize;
                sum ^= r.title.len();
                sum ^= r.views as usize;
                sum ^= r.published as usize;
                sum ^= r.author.as_u128() as usize;
                sum ^= r.created_at.as_micros() as usize;
            }
            (total, sum)
        },
    )
}

fn post_full_buffered(db: &Database, limit: usize) -> (usize, String) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        0,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

fn bench_doc(c: &mut Criterion) {
    for rows in ROWS {
        let (db, _dir) = populated_docs(rows);

        for limit in LIMITS {
            let label = format!("rows={rows}/limit={limit}");

            // Precomputed inputs for the isolated phases, so B is not billed for A.
            let (_, ids) = doc_phase_a(&db, limit);
            let page = doc_phase_b(&db, &ids);

            // The bytes are the contract (`tests/api_wire_test.rs`), and the two arms
            // being compared here must be producing them — a "win" from a shorter
            // response body would be no win at all. Asserted once per point, outside
            // the timed loop.
            let (_, buffered_body) = doc_full_buffered(&db, limit);
            assert_eq!(
                buffered_body,
                serde_json::to_string(&page).expect("serialize"),
                "post-#226 page bytes diverged from the pre-#226 page at {label}"
            );

            let mut g = c.benchmark_group("forgedb/list_page");

            // ARM ORDER IS THE METHOD, not cosmetics. Criterion runs arms in
            // registration order, so each compared pair is registered ADJACENT:
            // `full_path` then `full_buffered`, `scan_only` then `page_buffered`.
            // The two halves of a comparison are then measured seconds apart rather
            // than minutes, which is what keeps the pairing honest when something
            // else on the machine is competing for cores.
            g.bench_with_input(
                BenchmarkId::new("full_path", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| {
                        let (total, ids) = doc_phase_a(&db, limit);
                        let page = doc_phase_b(&db, &ids);
                        let body = serde_json::to_string(&page).expect("serialize");
                        std::hint::black_box((total, body))
                    });
                },
            );

            g.bench_with_input(
                BenchmarkId::new("full_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_full_buffered(&db, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(doc_phase_a(&db, limit)));
            });

            g.bench_with_input(
                BenchmarkId::new("page_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_page_buffered(&db, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("page_get", &label), &ids, |b, ids| {
                b.iter(|| std::hint::black_box(doc_phase_b(&db, ids)));
            });

            g.bench_with_input(BenchmarkId::new("serialize", &label), &page, |b, page| {
                b.iter(|| std::hint::black_box(serde_json::to_string(page).expect("serialize")));
            });

            g.finish();
        }
    }
}

/// The same six arms over `Post`, whose page gather is NOT empty (`author` is not in
/// `PostScanRef`) and which has one `string` field rather than four. In-run control on
/// the question `Doc` cannot answer: is the win an artifact of `Doc`'s field set?
fn bench_post_fk(c: &mut Criterion) {
    for rows in ROWS {
        let (db, _dir) = populated_posts(rows);

        for limit in LIMITS {
            let label = format!("rows={rows}/limit={limit}");

            let (_, ids) = post_phase_a(&db, limit);
            let page = post_phase_b(&db, &ids);

            let (_, buffered_body) = post_full_buffered(&db, limit);
            assert_eq!(
                buffered_body,
                serde_json::to_string(&page).expect("serialize"),
                "post-#226 page bytes diverged from the pre-#226 page at {label}"
            );

            let mut g = c.benchmark_group("forgedb/list_page_fk");

            g.bench_with_input(
                BenchmarkId::new("full_path", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| {
                        let (total, ids) = post_phase_a(&db, limit);
                        let page = post_phase_b(&db, &ids);
                        let body = serde_json::to_string(&page).expect("serialize");
                        std::hint::black_box((total, body))
                    });
                },
            );

            // Same adjacency as `bench_doc` — see the comment there.
            g.bench_with_input(
                BenchmarkId::new("full_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_full_buffered(&db, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(post_phase_a(&db, limit)));
            });

            g.bench_with_input(
                BenchmarkId::new("page_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_page_buffered(&db, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("page_get", &label), &ids, |b, ids| {
                b.iter(|| std::hint::black_box(post_phase_b(&db, ids)));
            });

            g.bench_with_input(BenchmarkId::new("serialize", &label), &page, |b, page| {
                b.iter(|| std::hint::black_box(serde_json::to_string(page).expect("serialize")));
            });

            g.finish();
        }
    }
}

criterion_group!(benches, bench_doc, bench_post_fk);
criterion_main!(benches);
