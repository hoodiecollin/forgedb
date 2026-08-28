#[allow(warnings)]
#[path = "../gen/database.rs"]
pub mod forgedb_generated;

#[cfg(feature = "router")]
pub use forgedb_generated::*;

#[cfg(feature = "router")]
#[allow(warnings)]
#[path = "../gen/api.rs"]
pub mod forgedb_api;

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
#[cfg(feature = "matrix")]
#[allow(warnings)]
#[path = "../gen/churn_probe/database.rs"]
pub mod v_churn_probe;

pub const SIZES: [usize; 3] = [1_000, 100_000, 1_000_000];

pub const LIST_SIZES: [usize; 3] = [1_000, 10_000, 100_000];

pub const LIST_CORE_ROWS: usize = 10_000;

pub const LIST_CORE_LIMIT: usize = 50;

pub const LIST_LIMITS: [usize; 2] = [LIST_CORE_LIMIT, 1_000];

pub const LIST_PROBE_VIEWS: u64 = 512;

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

pub const LIST_SHAPES: [(&str, &str); 4] = [
    ("unfiltered", ""),
    ("filtered_unindexed", " WHERE published"),
    ("filtered_indexed", " WHERE views = 512"),
    ("sorted", " ORDER BY views DESC"),
];

pub fn list_sql(where_order: &str, limit: usize, offset: usize) -> String {
    format!(
        "SELECT id, title, views, published, author, created_at FROM post{where_order} \
         LIMIT {limit} OFFSET {offset}"
    )
}

pub const N_TAGS: usize = 500;
pub const TAGS_PER_POST: usize = 3;

const BASE_TS: i64 = 1_700_000_000;

pub fn ts_from_seconds(secs: i64) -> forgedb_types::Timestamp {
    forgedb_types::Timestamp::from_micros(secs * 1_000_000)
}

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

pub struct Dataset {
    pub users: Vec<UserRow>,
    pub posts: Vec<PostRow>,
    pub tags: Vec<TagRow>,
    pub links: Vec<(usize, usize)>,
}

pub fn id_for(kind: u8, index: usize) -> [u8; 16] {
    let v = ((kind as u128) << 96) | (index as u128);
    uuid::Uuid::from_u128(v).into_bytes()
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

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
