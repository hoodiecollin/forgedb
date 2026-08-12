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
//! # #281 — the third path, and why phase A had to be split
//!
//! #226 left phase A whole because nothing it did could touch it. #281 can: with no
//! filter and no sort, `keep` accepts every row and `sort` reorders nothing, so the
//! page is knowable from the live row set alone and the full-table gather inside A is
//! pure waste. `__with_fast_page` gathers only `__rows[offset..offset+limit]`.
//!
//! That makes A's internal split load-bearing for the first time:
//!
//! ```text
//!   A_sel      collect the live row indices, sort them, drop the dead ones
//!   A_gather   gather + decode EVERY live row's scan columns, build one ScanRef each
//! ```
//!
//! #281 removes a fraction of `A_gather` and none of `A_sel`, so the ceiling is
//! `A_gather × (1 − p/r)` where `r` is the live row count and `p = min(limit, r − offset)`
//! is the page length — **not** `A` itself. Quoting `A` overstates the ceiling by
//! whatever fraction of it is `A_sel`, and that fraction is not small: it is the one
//! part of the request that is O(live rows) with no per-row column work to dilute it.
//!
//! `select_only` measures it in-run. It is `__with_fast_page(0, 0, ..)`: the selection
//! runs in full, then the gather is handed an empty slice and the view loop never
//! executes. It is named for what it measures and not for `A_sel` alone, because the
//! empty gather cannot be subtracted from outside — the generated struct's columns are
//! private, which is also why the standalone estimate made at Gate 2 was recorded as a
//! pre-registration figure rather than a published one. Both column kinds return an
//! empty buffer for an empty selection rather than erroring, so what it adds is a
//! branch and an allocation of zero.
//!
//! # The `(offset, limit)` grid
//!
//! Offset is threaded through every arm rather than pinned at 0, and the grid carries
//! `offset=10, limit=5` alongside the two page sizes #226 named. That point is not a
//! rounding-out: `p/r` is what governs the win, so a small page is #281's BEST regime,
//! while `limit=1000` on a 1,000-row table is `p = r` — the page is the whole table,
//! the bounded gather gathers everything, and the ceiling is exactly zero. A grid that
//! omitted either end would report the feature as uniformly good or uniformly marginal.
//!
//! # Not to be confused with `list_rest_bench.rs` (#282, scenario 21)
//!
//! Both files time a list request; they answer questions that are almost opposites, and
//! quoting one's number for the other's question is the failure this paragraph exists to
//! prevent.
//!
//! - **This file is ForgeDB-only, historical, and below the router.** Three code paths that
//!   all still exist in one binary (pre-#226, #226, #281), measured as a paired A/B with an
//!   in-run control. Its subject is *attribution* — which phase costs what, and how much of
//!   the ceiling a change realized. There is no HTTP anywhere in it.
//! - **`list_rest_bench.rs` is cross-engine, current, and spans the router.** Five engines on
//!   the shipped request, with ForgeDB measured at five rungs from typed rows up to a real
//!   TCP socket, so routing and the query string can be priced by subtraction. Its subject is
//!   *comparison*.
//!
//! Concretely: this file's `fast_buffered` arm and that file's `s1_rows_fast` arm both drive
//! `__with_fast_page`, so their absolute numbers are comparable — but a ratio taken here is a
//! share of a *phase*, and a ratio taken there is a share of a *request*. They are not
//! interchangeable, and the denominators differ by the whole envelope.
//!
//! # Reading the output
//!
//! Per `(rows, offset, limit)`, nine arms. Four are the pre-#226 path, two the
//! post-#226 one, three the #281 one:
//!
//! | arm | phases | what it is |
//! |---|---|---|
//! | `full_path` | A+B+C | pre-#226 request, the in-run control |
//! | `full_buffered` | A+B'+C' | post-#226 request — #281's control |
//! | `scan_only` | A | shared by both of those |
//! | `page_buffered` | A+B' | the #226 scope, views forced live, not serialized |
//! | `page_get` | B | what #226 removes — #226's ceiling |
//! | `serialize` | C | serde over `Vec<Model>` |
//! | `select_only` | A_sel | the fast path's fixed cost — #281 removes NONE of it |
//! | `fast_page` | A_sel+B'' | the #281 scope, views forced live |
//! | `fast_buffered` | A_sel+B''+C' | the #281 request |
//!
//! - **#281 realized win** = `full_buffered − fast_buffered`; **#281 ceiling** =
//!   `(scan_only − select_only) × (1 − p/r)`. Realized above ceiling means the arms are
//!   not doing the same work, exactly as for #226.
//! - `fast_buffered` and `full_buffered` must emit **byte-identical** bodies, asserted
//!   once per point outside the timed loop. A win from a shorter response is no win.
//!
//! # What it measured (#281's gate, one paired run)
//!
//! At `rows=10000, offset=0, limit=50` — the gate's named point — `full_buffered` →
//! `fast_buffered` was 1095.40 → 149.46 µs on `Doc` (86.4% of the request, 945.94 ±5.77)
//! and 419.14 → 150.01 µs on `Post` (64.2%, 269.13 ±1.53). Nowhere near a noise floor,
//! so the kill rule did not fire. `offset=10, limit=5` is stronger still (89.1% / 70.4%),
//! and `rows=1000, limit=1000` — `p = r`, where the model predicts exactly zero — measured
//! −0.64 ±3.05 µs and +2.55 ±0.88 µs: no win and no regression, as designed.
//!
//! Two caveats, both stated rather than rounded away:
//!
//! - `realized/ceiling` lands slightly **above** 100% at two points (102.9% and 100.2%,
//!   both on `Doc`). That is the ceiling *model* under-counting, not unequal work: it is
//!   built from `scan_only` (`__with_scan`), while the control runs `__with_page`, whose
//!   phase A additionally stores `__slot`, maps `__page_rows`, and sets up a second
//!   page-bounded gather — all of which `fast_buffered` also skips and none of which is
//!   inside `A_gather`. The bodies are asserted byte-identical at every point, and the
//!   102.9% point's ±38.38 band covers 100% on its own.
//! - `select_only` came out at 8.2–8.3 µs (1k) and 102–105 µs (10k), against a
//!   standalone replica of the same three operations measured at Gate 2 as 8.03 and
//!   104.94 µs. The agreement is close, but the in-run figure is the one to quote: the
//!   Gate 2 number was recorded as a pre-registration estimate precisely because it was
//!   a replica rather than the generated code.
//!
//! It is also what makes #226 and #281 legible together: after #226, the unfiltered
//! request was 97–99% phase A at 10k rows (`scan_only` 1066.40 of `full_buffered`
//! 1080.90 at `offset=10, limit=5`). The compound effect of both, `full_path` →
//! `fast_buffered`, is 9.67× on `Doc` and 3.95× on `Post` at the gate's point.
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
use uuid::Uuid;

/// Per-field body length. 200 chars is a realistic description/body field and keeps
/// the four columns' bytes well above the noise of the fixed columns beside them.
const BODY_LEN: usize = 200;

/// Table sizes. 1k and 10k rather than one size, because the ceiling is a *ratio*:
/// phase A is O(live rows) and phase B is O(limit), so the fraction necessarily moves
/// with table size. A single measurement would not reveal that, and a kill (or a
/// survival) argued from one table size would be arguing from a coincidence.
const ROWS: [usize; 2] = [1_000, 10_000];

/// The `(offset, limit)` grid.
///
/// The first two are `DEFAULT_LIMIT` and `MAX_LIMIT` from `forgedb-query-params` — the
/// page sizes #226's gate named. (Not imported: the detached bench project deliberately
/// does not depend on the substrate crate the generated `api.rs` links.)
///
/// The third is #281's, and it is the counter-regime rather than a third data point.
/// #281's win scales with `1 − p/r`, so `(10, 5)` is close to its best case while
/// `(0, 1000)` at `rows = 1000` is `p = r` — the page IS the table, the bounded gather
/// gathers all of it, and the predicted win there is exactly zero. Reporting only the
/// favourable end is how a feature gets shipped on a number that does not generalize.
const POINTS: [(usize, usize); 3] = [(0, 50), (0, 1_000), (10, 5)];

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

/// Base instant for `created_at`. `Timestamp`'s only unit is microseconds since
/// #254, which removed `Timestamp::from_seconds` outright rather than deprecating it.
/// #279 added `forgedb_benchmarks::ts_from_seconds` so the whole bench project has
/// exactly ONE seconds→micros conversion; this goes through it rather than spelling
/// the multiplication a second time.
const BASE_SECS: i64 = 1_700_000_000;

fn populated_posts(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    let author = id_of(AUTHOR, 0);
    db.user
        .insert(User {
            id: author,
            name: "bench".to_string(),
            email: "bench@example.com".to_string(),
            created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS),
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
                created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS + i as i64),
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
fn doc_phase_a(db: &Database, offset: usize, limit: usize) -> (usize, Vec<Uuid>) {
    db.doc.__with_scan(
        None,
        |_: &DocScanRef<'_>| true,
        |scan: &mut Vec<DocScanRef<'_>>| {
            let total = scan.len();
            // `Pagination::apply`'s arithmetic: clamp both ends to the length, and
            // saturate the addition so a large offset cannot overflow.
            let start = offset.min(scan.len());
            let end = offset.saturating_add(limit).min(scan.len());
            let ids: Vec<Uuid> = scan[start..end].iter().map(|r| r.id).collect();
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
fn doc_page_buffered(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.doc.__with_page(
        None,
        |_: &DocScanRef<'_>| true,
        |_: &mut Vec<DocScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[DocPageRef<'_>]| (total, doc_fold(page)),
    )
}

/// The checksum that keeps a page's view construction from being dead code. Shared by
/// the #226 and #281 scope arms so the two are folded identically — a difference in
/// what the fold reads would land in the subtraction between them and be read as a
/// difference between the paths.
fn doc_fold(page: &[DocPageRef<'_>]) -> usize {
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
    sum
}

/// Phases A+B'+C' — the whole post-#226 request path, exactly as the generated
/// handler runs it: everything inside the scope, and only an owned value escapes.
fn doc_full_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.doc.__with_page(
        None,
        |_: &DocScanRef<'_>| true,
        |_: &mut Vec<DocScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[DocPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

/// A_sel — the fast path with an EMPTY page. The live-row selection runs in full; the
/// gather is then handed a zero-length slice and the view loop never runs. See the
/// module docs for why the empty gather is not subtracted out.
fn doc_select_only(db: &Database) -> usize {
    db.doc.__with_fast_page(0, 0, |total: usize, _| total)
}

/// Phases A_sel+B'' — the #281 scope, views forced live by the same fold.
fn doc_fast_page(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.doc
        .__with_fast_page(offset, limit, |total: usize, page: &[DocPageRef<'_>]| {
            (total, doc_fold(page))
        })
}

/// Phases A_sel+B''+C' — the whole #281 request path.
fn doc_fast_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.doc
        .__with_fast_page(offset, limit, |total: usize, page: &[DocPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        })
}

fn post_phase_a(db: &Database, offset: usize, limit: usize) -> (usize, Vec<Uuid>) {
    db.post.__with_scan(
        None,
        |_: &PostScanRef<'_>| true,
        |scan: &mut Vec<PostScanRef<'_>>| {
            let total = scan.len();
            let start = offset.min(scan.len());
            let end = offset.saturating_add(limit).min(scan.len());
            let ids: Vec<Uuid> = scan[start..end].iter().map(|r| r.id).collect();
            (total, ids)
        },
    )
}

fn post_phase_b(db: &Database, ids: &[Uuid]) -> Vec<Post> {
    ids.iter().filter_map(|id| db.post.get(*id)).collect()
}

fn post_page_buffered(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| (total, post_fold(page)),
    )
}

/// `doc_fold`'s counterpart — see the note there on why both scope arms share it.
fn post_fold(page: &[PostPageRef<'_>]) -> usize {
    let mut sum = 0usize;
    for r in page {
        sum ^= r.id.as_u128() as usize;
        sum ^= r.title.len();
        sum ^= r.views as usize;
        sum ^= r.published as usize;
        sum ^= r.author.as_u128() as usize;
        sum ^= r.created_at.as_micros() as usize;
    }
    sum
}

fn post_full_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

fn post_select_only(db: &Database) -> usize {
    db.post.__with_fast_page(0, 0, |total: usize, _| total)
}

fn post_fast_page(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.post
        .__with_fast_page(offset, limit, |total: usize, page: &[PostPageRef<'_>]| {
            (total, post_fold(page))
        })
}

fn post_fast_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post
        .__with_fast_page(offset, limit, |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        })
}

fn bench_doc(c: &mut Criterion) {
    for rows in ROWS {
        let (db, _dir) = populated_docs(rows);

        for (offset, limit) in POINTS {
            let label = format!("rows={rows}/off={offset}/limit={limit}");

            // Precomputed inputs for the isolated phases, so B is not billed for A.
            let (_, ids) = doc_phase_a(&db, offset, limit);
            let page = doc_phase_b(&db, &ids);

            // The bytes are the contract (`tests/api_wire_test.rs`), and the arms
            // being compared here must be producing them — a "win" from a shorter
            // response body would be no win at all. Asserted once per point, outside
            // the timed loop, for BOTH the #226 and the #281 path against the same
            // pre-#226 reference.
            let (_, buffered_body) = doc_full_buffered(&db, offset, limit);
            let reference = serde_json::to_string(&page).expect("serialize");
            assert_eq!(
                buffered_body, reference,
                "post-#226 page bytes diverged from the pre-#226 page at {label}"
            );
            let (fast_total, fast_body) = doc_fast_buffered(&db, offset, limit);
            assert_eq!(
                fast_body, reference,
                "#281 fast page bytes diverged from the pre-#226 page at {label}"
            );
            assert_eq!(
                fast_total, rows,
                "#281 `total` must be the live row count, not the page length, at {label}"
            );

            let mut g = c.benchmark_group("forgedb/list_page");

            // ARM ORDER IS THE METHOD, not cosmetics. Criterion runs arms in
            // registration order, so each compared pair is registered ADJACENT:
            // `full_path` then `full_buffered`, `scan_only` then `page_buffered`.
            // The two halves of a comparison are then measured seconds apart rather
            // than minutes, which is what keeps the pairing honest when something
            // else on the machine is competing for cores.
            g.bench_with_input(BenchmarkId::new("full_path", &label), &limit, |b, &limit| {
                b.iter(|| {
                    let (total, ids) = doc_phase_a(&db, offset, limit);
                    let page = doc_phase_b(&db, &ids);
                    let body = serde_json::to_string(&page).expect("serialize");
                    std::hint::black_box((total, body))
                });
            });

            g.bench_with_input(
                BenchmarkId::new("full_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_full_buffered(&db, offset, limit)));
                },
            );

            // #281's control is `full_buffered`, not `full_path`: the branch is taken
            // instead of the post-#226 scan path, so that is the arm it replaces.
            // Registered immediately after it for the same adjacency reason.
            g.bench_with_input(
                BenchmarkId::new("fast_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_fast_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(doc_phase_a(&db, offset, limit)));
            });

            g.bench_with_input(
                BenchmarkId::new("page_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_page_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("fast_page", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(doc_fast_page(&db, offset, limit)));
            });

            // `scan_only − select_only` is A_gather, the only phase #281 shrinks.
            // Registered adjacent to `scan_only` for the same reason every other pair
            // is: the two halves of a subtraction are measured back to back.
            g.bench_with_input(BenchmarkId::new("select_only", &label), &limit, |b, _| {
                b.iter(|| std::hint::black_box(doc_select_only(&db)));
            });

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

        for (offset, limit) in POINTS {
            let label = format!("rows={rows}/off={offset}/limit={limit}");

            let (_, ids) = post_phase_a(&db, offset, limit);
            let page = post_phase_b(&db, &ids);

            let (_, buffered_body) = post_full_buffered(&db, offset, limit);
            let reference = serde_json::to_string(&page).expect("serialize");
            assert_eq!(
                buffered_body, reference,
                "post-#226 page bytes diverged from the pre-#226 page at {label}"
            );
            let (fast_total, fast_body) = post_fast_buffered(&db, offset, limit);
            assert_eq!(
                fast_body, reference,
                "#281 fast page bytes diverged from the pre-#226 page at {label}"
            );
            assert_eq!(
                fast_total, rows,
                "#281 `total` must be the live row count, not the page length, at {label}"
            );

            let mut g = c.benchmark_group("forgedb/list_page_fk");

            g.bench_with_input(BenchmarkId::new("full_path", &label), &limit, |b, &limit| {
                b.iter(|| {
                    let (total, ids) = post_phase_a(&db, offset, limit);
                    let page = post_phase_b(&db, &ids);
                    let body = serde_json::to_string(&page).expect("serialize");
                    std::hint::black_box((total, body))
                });
            });

            // Same adjacency as `bench_doc` — see the comments there.
            g.bench_with_input(
                BenchmarkId::new("full_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_full_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(
                BenchmarkId::new("fast_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_fast_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(post_phase_a(&db, offset, limit)));
            });

            g.bench_with_input(
                BenchmarkId::new("page_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_page_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("fast_page", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(post_fast_page(&db, offset, limit)));
            });

            g.bench_with_input(BenchmarkId::new("select_only", &label), &limit, |b, _| {
                b.iter(|| std::hint::black_box(post_select_only(&db)));
            });

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
