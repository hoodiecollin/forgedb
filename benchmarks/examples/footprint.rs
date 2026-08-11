//! Footprint report (scenario 18): on-disk bytes per N rows for each engine over
//! the SAME seeded corpus, plus ForgeDB's update-churn bloat before/after
//! compaction (superseding-version append grows storage until `compact()` reclaims).
//!
//! This is a SIZE report, not a Criterion timing, so it lives as an example (which,
//! unlike a plain bin, may use the bench dev-deps rusqlite/redb/duckdb). Run:
//!   cargo run --manifest-path benchmarks/Cargo.toml --example footprint --release
//!
//! Every engine is loaded via its simplest durable write path and then measured by
//! summing the on-disk file sizes of its data directory/file — an apples-to-apples
//! "how many bytes on disk for this dataset" number. Durability/WAL state is
//! normalized (SQLite checkpointed to its main db, ForgeDB checkpointed) so the
//! comparison is steady-state data size, not transient journal size.

use std::path::Path;

use duckdb::{params as dparams, Connection as DuckConn};
use forgedb_benchmarks::forgedb_generated::{Database, Post, Tag, User};
use forgedb_benchmarks::{dataset, Dataset, PostRow, TagRow, UserRow};
use forgedb_benchmarks::ts_from_seconds;
use redb::{Database as Redb, Durability, MultimapTableDefinition, TableDefinition};
use rusqlite::{params, Connection};
use uuid::Uuid;

const N_USERS: usize = 1_000;
const N_POSTS: usize = 10_000;

// --- helpers -----------------------------------------------------------------
fn dir_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = p.metadata() {
                total += m.len();
            }
        }
    }
    total
}

fn human(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.2} MB", bytes as f64 / (1 << 20) as f64)
    } else if bytes >= 1 << 10 {
        format!("{:.1} KB", bytes as f64 / (1 << 10) as f64)
    } else {
        format!("{bytes} B")
    }
}

// --- ForgeDB -----------------------------------------------------------------
fn user_of(r: &UserRow) -> User {
    User {
        id: Uuid::from_bytes(r.id),
        name: r.name.clone(),
        email: r.email.clone(),
        created_at: ts_from_seconds(r.created_at),
        posts: (),
    }
}
fn post_of(r: &PostRow) -> Post {
    Post {
        id: Uuid::from_bytes(r.id),
        title: r.title.clone(),
        views: r.views,
        published: r.published,
        author: Uuid::from_bytes(r.author),
        created_at: ts_from_seconds(r.created_at),
        tags: (),
    }
}
fn tag_of(r: &TagRow) -> Tag {
    Tag { id: Uuid::from_bytes(r.id), name: r.name.clone(), posts: () }
}

fn forgedb_size(data: &Dataset) -> u64 {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    for u in &data.users {
        db.user.insert(user_of(u)).unwrap();
    }
    for p in &data.posts {
        db.post.insert(post_of(p)).unwrap();
    }
    for t in &data.tags {
        db.tag.insert(tag_of(t)).unwrap();
    }
    for &(p, t) in &data.links {
        db.link_post_tag(Uuid::from_bytes(data.posts[p].id), Uuid::from_bytes(data.tags[t].id));
    }
    db.checkpoint();
    dir_size(dir.path())
}

/// Update-churn bloat: insert `N_USERS`, then update ONE user `churn` times.
/// Each update appends a superseding version (storage grows), until `compact()`
/// reclaims the dead versions. Returns (bytes_before_compact, bytes_after_compact).
fn forgedb_churn(churn: usize) -> (u64, u64) {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    let base = dataset(N_USERS, 0);
    for u in &base.users {
        db.user.insert(user_of(u)).unwrap();
    }
    let target = Uuid::from_bytes(base.users[0].id);
    for i in 0..churn {
        let mut u = user_of(&base.users[0]);
        u.name = format!("churn-{i}");
        db.user.update(target, u).unwrap();
    }
    db.checkpoint();
    let before = dir_size(dir.path());
    db.compact();
    db.checkpoint();
    let after = dir_size(dir.path());
    (before, after)
}

// --- SQLite ------------------------------------------------------------------
fn sqlite_size(data: &Dataset) -> u64 {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bench.db");
    let mut conn = Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("../schema.sql")).unwrap();
    let tx = conn.transaction().unwrap();
    for u in &data.users {
        tx.execute(
            "INSERT INTO user (id, name, email, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![&u.id[..], u.name, u.email, u.created_at],
        )
        .unwrap();
    }
    for p in &data.posts {
        tx.execute(
            "INSERT INTO post (id, title, views, published, author, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![&p.id[..], p.title, p.views as i64, p.published as i64, &p.author[..], p.created_at],
        )
        .unwrap();
    }
    for t in &data.tags {
        tx.execute("INSERT INTO tag (id, name) VALUES (?1, ?2)", params![&t.id[..], t.name]).unwrap();
    }
    for &(p, t) in &data.links {
        tx.execute(
            "INSERT INTO post_tag_link (post_id, tag_id) VALUES (?1, ?2)",
            params![&data.posts[p].id[..], &data.tags[t].id[..]],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    drop(conn); // flush + close so only the steady-state .db file remains
    dir_size(dir.path())
}

// --- redb --------------------------------------------------------------------
const USER: TableDefinition<&[u8], &[u8]> = TableDefinition::new("user");
const POST: TableDefinition<&[u8], &[u8]> = TableDefinition::new("post");
const TAG: TableDefinition<&[u8], &[u8]> = TableDefinition::new("tag");
const EMAIL_IDX: TableDefinition<&str, &[u8]> = TableDefinition::new("email_idx");
const AUTHOR_IDX: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("author_idx");
const POST_TAG: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("post_tag");

fn pack_user(name: &str, email: &str, created_at: i64) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&created_at.to_le_bytes());
    v.extend_from_slice(&(name.len() as u32).to_le_bytes());
    v.extend_from_slice(name.as_bytes());
    v.extend_from_slice(email.as_bytes());
    v
}
fn pack_post(title: &str, views: u64, published: bool, author: &[u8; 16], created_at: i64) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&views.to_le_bytes());
    v.extend_from_slice(&created_at.to_le_bytes());
    v.push(published as u8);
    v.extend_from_slice(author);
    v.extend_from_slice(&(title.len() as u32).to_le_bytes());
    v.extend_from_slice(title.as_bytes());
    v
}

fn redb_size(data: &Dataset) -> u64 {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bench.redb");
    let db = Redb::create(&path).unwrap();
    let mut tx = db.begin_write().unwrap();
    tx.set_durability(Durability::Immediate);
    {
        let mut users = tx.open_table(USER).unwrap();
        let mut emails = tx.open_table(EMAIL_IDX).unwrap();
        for u in &data.users {
            users.insert(&u.id[..], pack_user(&u.name, &u.email, u.created_at).as_slice()).unwrap();
            emails.insert(u.email.as_str(), &u.id[..]).unwrap();
        }
        let mut posts = tx.open_table(POST).unwrap();
        let mut authors = tx.open_multimap_table(AUTHOR_IDX).unwrap();
        for p in &data.posts {
            posts.insert(&p.id[..], pack_post(&p.title, p.views, p.published, &p.author, p.created_at).as_slice()).unwrap();
            authors.insert(&p.author[..], &p.id[..]).unwrap();
        }
        let mut tags = tx.open_table(TAG).unwrap();
        for t in &data.tags {
            tags.insert(&t.id[..], t.name.as_bytes()).unwrap();
        }
        let mut post_tags = tx.open_multimap_table(POST_TAG).unwrap();
        for &(p, t) in &data.links {
            post_tags.insert(&data.posts[p].id[..], &data.tags[t].id[..]).unwrap();
        }
    }
    tx.commit().unwrap();
    drop(db);
    dir_size(dir.path())
}

// --- DuckDB ------------------------------------------------------------------
fn duckdb_size(data: &Dataset) -> u64 {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bench.duckdb");
    let conn = DuckConn::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE user (id BLOB PRIMARY KEY, name VARCHAR, email VARCHAR UNIQUE, created_at BIGINT);
         CREATE TABLE post (id BLOB PRIMARY KEY, title VARCHAR, views UBIGINT, published BOOLEAN, author BLOB, created_at BIGINT);
         CREATE INDEX post_author_idx ON post(author);
         CREATE TABLE tag (id BLOB PRIMARY KEY, name VARCHAR);
         CREATE TABLE post_tag_link (post_id BLOB, tag_id BLOB);
         CREATE INDEX ptl_post_idx ON post_tag_link(post_id);",
    )
    .unwrap();
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for u in &data.users {
        conn.execute(
            "INSERT INTO user (id,name,email,created_at) VALUES (?,?,?,?)",
            dparams![&u.id[..], u.name, u.email, u.created_at],
        )
        .unwrap();
    }
    for p in &data.posts {
        conn.execute(
            "INSERT INTO post (id,title,views,published,author,created_at) VALUES (?,?,?,?,?,?)",
            dparams![&p.id[..], p.title, p.views, p.published, &p.author[..], p.created_at],
        )
        .unwrap();
    }
    for t in &data.tags {
        conn.execute("INSERT INTO tag (id,name) VALUES (?,?)", dparams![&t.id[..], t.name]).unwrap();
    }
    for &(p, t) in &data.links {
        conn.execute(
            "INSERT INTO post_tag_link (post_id,tag_id) VALUES (?,?)",
            dparams![&data.posts[p].id[..], &data.tags[t].id[..]],
        )
        .unwrap();
    }
    conn.execute_batch("COMMIT; CHECKPOINT;").unwrap();
    drop(conn);
    dir_size(dir.path())
}

fn main() {
    let data = dataset(N_USERS, N_POSTS);
    let n = N_USERS + N_POSTS + data.tags.len() + data.links.len();
    println!(
        "On-disk footprint for the shared corpus: {N_USERS} users + {N_POSTS} posts + \
         {} tags + {} M2M links = {n} rows\n",
        data.tags.len(),
        data.links.len()
    );

    let forgedb = forgedb_size(&data);
    let sqlite = sqlite_size(&data);
    let redb = redb_size(&data);
    println!("  {:<10} {:>12}", "engine", "on-disk");
    println!("  {:<10} {:>12}", "ForgeDB", human(forgedb));
    println!("  {:<10} {:>12}", "SQLite", human(sqlite));
    println!("  {:<10} {:>12}", "redb", human(redb));
    // DuckDB is the slow-to-build engine (bundled C++); build it last so the
    // fast engines already printed if the DuckDB compile is what you're waiting on.
    let duck = duckdb_size(&data);
    println!("  {:<10} {:>12}", "DuckDB", human(duck));

    println!("\nForgeDB update-churn bloat (2000 updates to one user, then compact):");
    let (before, after) = forgedb_churn(2_000);
    println!("  before compact: {:>12}", human(before));
    println!("  after compact:  {:>12}", human(after));
    println!(
        "  reclaimed:      {:>12}  ({:.0}% of the pre-compact bytes)",
        human(before.saturating_sub(after)),
        if before > 0 { (before - after) as f64 * 100.0 / before as f64 } else { 0.0 }
    );
}
