//! Shared benchmark fixtures: one deterministic, seeded data source consumed by
//! every engine suite, plus the ForgeDB-generated database as a module. Keeping
//! generation here (engine-agnostic plain rows) guarantees ForgeDB and SQLite
//! bench the *same* records, so their Criterion groups line up. See
//! `docs/BENCHMARKS.md`.

// ===========================================================================
// SECTION 1 — the default generated project (`benchmarks/gen/`). TRACKED in git,
// re-emitted by `make bench-regen`. Every declaration here is UNCONDITIONAL,
// because the file it points at is committed and therefore present in a fresh
// clone.
//
// The gate criterion for a NEW declaration is NOT merely "is the file tracked".
// It is: **does declaring it make this library depend on something a plain
// `cargo bench` should not have to build?** Gitignored-ness is one instance of
// that (a missing file cannot compile at all — the #279 bug); a heavy dependency
// tree is another, and it is the one that bites next. #282 adds the generated
// `api.rs`, whose emitted `use` statements pull axum, tokio, utoipa-axum and
// tower-http in as NORMAL deps of this lib (see `crates/codegen/src/api.rs` and
// the scaffold pins in `src/commands/init.rs`) — so every bench target, even
// `make bench-sqlite`, would start paying that compile cost. Tracked, but not
// free: gate it behind its own feature and have only the targets that need the
// router enable it. Do NOT copy a declaration out of section 2 either; its `cfg`
// exists only because those files are gitignored.
// ===========================================================================

/// The ForgeDB-GENERATED database code (`benchmarks/gen/database.rs`), compiled
/// as part of this crate so the bench links the real generated `Database` API.
/// Regenerate with `make bench-regen` after any codegen change — this module
/// compiling is itself a codegen guard (snapshot pass ≠ output compiles).
#[allow(warnings)]
#[path = "../gen/database.rs"]
pub mod forgedb_generated;

/// Re-exported at the crate ROOT because the generated `api.rs` opens with
/// `use super::*;` and names dozens of distinct `super::` items (`super::Database`,
/// `super::Post`, `super::PostScanRef`, `super::PostPageRef`, …). `include!` is barred:
/// the generated file carries the INNER attribute `#![allow(dead_code,
/// unused_imports)]`, and an inner attribute inside an inline `mod { }` is E0753 —
/// `src/commands/init.rs` records the same bar and the same `mod database; use
/// database::*;` fix. Gated with the module below so a plain `cargo bench` sees no
/// glob at all.
///
/// A glob import LOSES to an explicit item, so a generated type colliding with one of
/// this crate's own (`Dataset`, `SIZES`, `UserRow`, `id_for`, …) would silently resolve
/// `api.rs`'s `super::` to the wrong thing. Zero collisions today; re-check after any
/// change to `bench.forge`.
#[cfg(feature = "router")]
pub use forgedb_generated::*;

/// The ForgeDB-GENERATED REST router (`benchmarks/gen/api.rs`) — #282's S3/S4 arms.
///
/// TRACKED, like `database.rs`, but **cfg-gated**: a third category neither section
/// heading names. Section 1's rule is "tracked ⇒ unconditional"; section 1's *criterion*
/// is "does declaring it make this library depend on something a plain `cargo bench`
/// should not have to build?" — and this one does (axum, tokio, tower-http,
/// utoipa-axum: +78 packages on the normal graph). The criterion wins over the heading.
/// See `[[bench]] list_rest_bench`'s `required-features` and `make bench-deps-check`.
#[cfg(feature = "router")]
#[allow(warnings)]
#[path = "../gen/api.rs"]
pub mod forgedb_api;

// ===========================================================================
// SECTION 2 — config-matrix variants (`benchmarks/gen/<variant>/`). GITIGNORED,
// so they exist only after `make bench-regen-matrix`, and are compiled ONLY
// under `--features matrix` (#279). Declaring them unconditionally made the
// bench LIBRARY depend on untracked files, which broke every bench target in a
// clean clone — not just the matrix one. Anything linking a variant must state
// that: `required-features = ["matrix"]` on the target, or a `cfg` at the use
// site (`examples/workload/main.rs`, whose `--var-sweep` mode needs
// `v_churn_probe`).
// ===========================================================================

/// Config-matrix variants (epic #126): the SAME `bench.forge` generated under a
/// range of `forgedb.toml` configs, each a separate module, so the matrix bench
/// (`benches/matrix_bench.rs`) can measure how a generate-time knob shifts the
/// write path. Regenerate all of these with `make bench-regen-matrix`. Each
/// module is byte-different only where its knob bakes in (fsync policy, broker
/// presence, thresholds, changefeed capacity) — the schema is identical.
/// (Top-level modules, not nested under a `variants` module: `#[path]` for an
/// inline nested module resolves through a `src/<mod>/` dir that does not exist.)
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/default/database.rs"]
pub mod v_default;
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/fsync_never/database.rs"]
pub mod v_fsync_never;
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/replication_on/database.rs"]
pub mod v_replication_on;
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/compaction_off/database.rs"]
pub mod v_compaction_off;
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/compaction_low/database.rs"]
pub mod v_compaction_low;
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/changefeed_small/database.rs"]
pub mod v_changefeed_small;
/// #218 high-amplification read probe: `compaction = false` + `fsync = "never"`.
/// Compaction-off lifts the `1 + 4000/live_rows` auto-compaction ceiling so the
/// amplification ladder can reach 8x/16x/32x at all; fsync-never keeps the preload
/// affordable there. Read-path measurements only — see `configs/churn_probe.toml`.
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/churn_probe/database.rs"]
pub mod v_churn_probe;

/// Row counts every scaling scenario sweeps (small / medium / large).
pub const SIZES: [usize; 3] = [1_000, 100_000, 1_000_000];

/// Row counts the #282 REST-list size sweep walks. Deliberately NOT `SIZES`, which is
/// `[1_000, 100_000, 1_000_000]` — scenario 21's accepted grid is {1k, 10k, 100k}, and
/// 10k is the core-grid size every one of the five engine suites already populates
/// (`READ_POSTS = 10_000` in all of them). A million-row REST list sweep is a different
/// experiment, not this one.
pub const LIST_SIZES: [usize; 3] = [1_000, 10_000, 100_000];

/// The #282 core grid point — `READ_POSTS` in every one of the five engine suites.
pub const LIST_CORE_ROWS: usize = 10_000;

/// `PAGE_DEFAULT_LIMIT` from the generated handler — what a bare `GET /api/post` sends.
pub const LIST_CORE_LIMIT: usize = 50;

/// The limit sweep: `PAGE_DEFAULT_LIMIT` and `PAGE_MAX_LIMIT`.
pub const LIST_LIMITS: [usize; 2] = [LIST_CORE_LIMIT, 1_000];

/// A `views` value present in every corpus size, for the filtered-indexed shape. Chosen
/// inside `LIST_SIZES[0]` so the same query is valid at every point in the sweep.
pub const LIST_PROBE_VIEWS: u64 = 512;

/// Every `(rows, limit)` point scenario 21 visits, as ONE list.
///
/// The Criterion sweeps do NOT consume this — they walk `LIST_SIZES` and `LIST_LIMITS` in
/// two separate groups, mirroring the ForgeDB ladder's group names, which is also what
/// lets the core point appear in both without a duplicate-ID panic. This is for consumers
/// that want the point set flat and un-grouped (the S2/S3 wire-parity guard).
///
/// Same argument as [`ts_from_seconds`]: five bench sources deriving the grid
/// independently is five chances to disagree, and a grid that disagrees does not fail —
/// it silently compares a 10k page against a 100k one, which looks like an engine
/// difference. The sweeps are a size sweep at the core limit plus a limit sweep at the
/// core size, so the core point itself appears exactly once.
pub fn list_grid() -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = LIST_SIZES.iter().map(|r| (*r, LIST_CORE_LIMIT)).collect();
    out.extend(
        LIST_LIMITS
            .iter()
            .filter(|l| **l != LIST_CORE_LIMIT)
            .map(|l| (LIST_CORE_ROWS, *l)),
    );
    out
}

// --- Scenario 21 (#282): the REST list endpoint, S1 and S2 -------------------
//
// S1 = "the page's rows in the serializable host-language form this engine's shipped read
// path actually produces" -- for a cursor engine that is `query_map` into a struct, which
// must COPY the strings ForgeDB's page view borrows out of its column buffer. That
// asymmetry is real, is a property of the shipped paths, and is reported rather than
// hidden (docs/BENCHMARKS.md); the cross-engine headline is S2.
//
// S2 = the JSON array bytes over the SAME FIELD SET, so `S2 - S1` is the same added work
// in every suite. Comparing ForgeDB's JSON against another engine's typed rows would
// penalise us for serialization the others skip -- the mirror of the fsync-per-row mistake.
//
// NOTE on `created_at`: the field serializes as an RFC 3339 string on every ForgeDB wire
// surface, so `PostJson` carries `forgedb_types::Timestamp` here too. Using ForgeDB's type
// in the SQLite arm looks odd and is deliberate -- if this arm emitted a bare integer, its
// serde term would be cheaper than ForgeDB's for a reason that has nothing to do with
// either engine, and `S2 - S1` would stop being the same added work.

/// The page row, materialized. Field set and order match the generated `PostPageRef`, so
/// `S2 - S1` is the serialization of the SAME payload in every suite.
///
/// `created_at` is a `forgedb_types::Timestamp` even in the non-ForgeDB suites, and that is
/// deliberate rather than an oversight: the field serializes as an RFC 3339 string on every
/// ForgeDB wire surface, so an arm emitting a bare integer here would have a cheaper serde
/// term for a reason that has nothing to do with either engine. `tags: ()` serializes as
/// `null`, matching the page view's un-traversed relation.
#[derive(serde::Serialize)]
pub struct PostJson {
    pub id: uuid::Uuid,
    pub title: String,
    pub views: u64,
    pub published: bool,
    pub author: uuid::Uuid,
    pub created_at: forgedb_types::Timestamp,
    pub tags: (),
}

pub fn uuid_of(bytes: Vec<u8>) -> uuid::Uuid {
    let mut b = [0u8; 16];
    b.copy_from_slice(&bytes[..16]);
    uuid::Uuid::from_bytes(b)
}

/// The four query shapes, as the `WHERE`/`ORDER BY` each engine's mirror runs.
///
/// **No `ORDER BY` on the unfiltered shape, for anyone.** Adding `ORDER BY id` to make row
/// sets match across engines would silently convert it into the *sorted* shape and penalise
/// the SQL engines. The consequence -- engines may return different 50 rows -- is stated in
/// docs/BENCHMARKS.md rather than glossed.
pub const LIST_SHAPES: [(&str, &str); 4] = [
    ("unfiltered", ""),
    // `WHERE published`, NOT `WHERE published = 1`. SQLite stores the bool as 0/1 INTEGER
    // and accepts either; DuckDB has a real BOOLEAN and coerces `= 1` silently; PostgreSQL
    // has a real BOOLEAN and REFUSES it (`operator does not exist: boolean = integer`).
    // Bare truthiness is the one spelling all three accept, so this stays ONE string rather
    // than three per-dialect copies that could drift into three different predicates.
    ("filtered_unindexed", " WHERE published"),
    // Equality only: the generated REST filter has no `>=` spelling (#284), so every SQL
    // mirror is `= N`. Selects ~0-1 rows in this corpus -- an O(1) probe of the index path,
    // not a 50-row filtered page.
    ("filtered_indexed", " WHERE views = 512"),
    ("sorted", " ORDER BY views DESC"),
];

pub fn list_sql(where_order: &str, limit: usize, offset: usize) -> String {
    format!(
        "SELECT id, title, views, published, author, created_at FROM post{where_order} \
         LIMIT {limit} OFFSET {offset}"
    )
}

/// Distinct tags in the corpus, and how many each post links to (M2M fan-out).
pub const N_TAGS: usize = 500;
pub const TAGS_PER_POST: usize = 3;

/// Base unix-seconds for `created_at`, so timestamps are stable across runs.
const BASE_TS: i64 = 1_700_000_000;

/// The engine-agnostic rows below keep `created_at` in unix **seconds** (see
/// [`BASE_TS`]) because that is what SQLite / redb / DuckDB / PG store. ForgeDB's
/// `Timestamp` is **microseconds** since #254 (`6106cc0` removed the
/// `Timestamp::from_seconds` every bench source used to call), so the mapping into
/// the generated types lives here exactly once — five bench sources converting
/// independently is five chances to disagree about the unit, which would silently
/// change what the cross-engine comparison is comparing.
pub fn ts_from_seconds(secs: i64) -> forgedb_types::Timestamp {
    forgedb_types::Timestamp::from_micros(secs * 1_000_000)
}

/// Engine-agnostic rows. Each engine maps these into its own types, so the bytes
/// on disk differ only by the engine's encoding — never by the data.
#[derive(Clone)]
pub struct UserRow {
    pub id: [u8; 16],
    pub name: String,
    pub email: String,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct PostRow {
    pub id: [u8; 16],
    pub title: String,
    pub views: u64,
    pub published: bool,
    pub author: [u8; 16],
    pub created_at: i64,
}

#[derive(Clone)]
pub struct TagRow {
    pub id: [u8; 16],
    pub name: String,
}

/// A fully-generated corpus: users, posts, tags, and the post↔tag link pairs
/// (indices into `posts` / `tags`).
pub struct Dataset {
    pub users: Vec<UserRow>,
    pub posts: Vec<PostRow>,
    pub tags: Vec<TagRow>,
    pub links: Vec<(usize, usize)>,
}

/// Deterministic 16-byte id from a `kind` tag (1=user, 2=post, 3=tag) + index,
/// so both engines store byte-identical ids and lookups can target a known row.
pub fn id_for(kind: u8, index: usize) -> [u8; 16] {
    let v = ((kind as u128) << 96) | (index as u128);
    uuid::Uuid::from_u128(v).into_bytes()
}

/// splitmix64 — a tiny deterministic PRNG (no `rand` dependency) for `views`.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Build a corpus of `n_posts` posts over `n_users` users and [`N_TAGS`] tags.
/// Every field is a pure function of its index (+ a seeded PRNG for `views`), so
/// the dataset is identical across runs and across engines.
pub fn dataset(n_users: usize, n_posts: usize) -> Dataset {
    let n_tags = N_TAGS.min(n_posts.max(1));
    let mut rng = 0x1234_5678_9ABC_DEF0u64;

    let users = (0..n_users)
        .map(|i| UserRow {
            id: id_for(1, i),
            name: format!("user{i}"),
            email: format!("user{i}@example.com"),
            created_at: BASE_TS + i as i64,
        })
        .collect();

    let posts = (0..n_posts)
        .map(|i| PostRow {
            id: id_for(2, i),
            title: format!("post title number {i}"),
            views: splitmix64(&mut rng) % 100_000,
            published: i % 2 == 0,
            author: id_for(1, i % n_users.max(1)),
            created_at: BASE_TS + i as i64,
        })
        .collect();

    let tags = (0..n_tags)
        .map(|i| TagRow {
            id: id_for(3, i),
            name: format!("tag{i}"),
        })
        .collect();

    // Each post links to TAGS_PER_POST distinct tags, spread deterministically.
    let mut links = Vec::with_capacity(n_posts * TAGS_PER_POST);
    for p in 0..n_posts {
        for k in 0..TAGS_PER_POST {
            let t = (p * 7 + k * 101) % n_tags;
            links.push((p, t));
        }
    }

    Dataset {
        users,
        posts,
        tags,
        links,
    }
}
