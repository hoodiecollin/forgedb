//! Shared benchmark fixtures: one deterministic, seeded data source consumed by
//! every engine suite, plus the ForgeDB-generated database as a module. Keeping
//! generation here (engine-agnostic plain rows) guarantees ForgeDB and SQLite
//! bench the *same* records, so their Criterion groups line up. See
//! `docs/BENCHMARKS.md`.

/// The ForgeDB-GENERATED database code (`benchmarks/gen/database.rs`), compiled
/// as part of this crate so the bench links the real generated `Database` API.
/// Regenerate with `make bench-regen` after any codegen change — this module
/// compiling is itself a codegen guard (snapshot pass ≠ output compiles).
#[allow(warnings)]
#[path = "../gen/database.rs"]
pub mod forgedb_generated;

/// Config-matrix variants (epic #126): the SAME `bench.forge` generated under a
/// range of `forgedb.toml` configs, each a separate module, so the matrix bench
/// (`benches/matrix_bench.rs`) can measure how a generate-time knob shifts the
/// write path. Regenerate all of these with `make bench-regen-matrix`. Each
/// module is byte-different only where its knob bakes in (fsync policy, broker
/// presence, thresholds, changefeed capacity) — the schema is identical.
/// (Top-level modules, not nested under a `variants` module: `#[path]` for an
/// inline nested module resolves through a `src/<mod>/` dir that does not exist.)
#[allow(warnings)]
#[path = "../gen/default/database.rs"]
pub mod v_default;
#[allow(warnings)]
#[path = "../gen/fsync_never/database.rs"]
pub mod v_fsync_never;
#[allow(warnings)]
#[path = "../gen/replication_on/database.rs"]
pub mod v_replication_on;
#[allow(warnings)]
#[path = "../gen/compaction_off/database.rs"]
pub mod v_compaction_off;
#[allow(warnings)]
#[path = "../gen/compaction_low/database.rs"]
pub mod v_compaction_low;
#[allow(warnings)]
#[path = "../gen/changefeed_small/database.rs"]
pub mod v_changefeed_small;
/// #218 high-amplification read probe: `compaction = false` + `fsync = "never"`.
/// Compaction-off lifts the `1 + 4000/live_rows` auto-compaction ceiling so the
/// amplification ladder can reach 8x/16x/32x at all; fsync-never keeps the preload
/// affordable there. Read-path measurements only — see `configs/churn_probe.toml`.
#[allow(warnings)]
#[path = "../gen/churn_probe/database.rs"]
pub mod v_churn_probe;

/// Row counts every scaling scenario sweeps (small / medium / large).
pub const SIZES: [usize; 3] = [1_000, 100_000, 1_000_000];

/// Distinct tags in the corpus, and how many each post links to (M2M fan-out).
pub const N_TAGS: usize = 500;
pub const TAGS_PER_POST: usize = 3;

/// Base unix-seconds for `created_at`, so timestamps are stable across runs.
const BASE_TS: i64 = 1_700_000_000;

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
