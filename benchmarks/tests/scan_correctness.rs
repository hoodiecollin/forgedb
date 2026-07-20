//! #168 correctness: the generated bulk-buffered `__scan_all()` must return
//! exactly the same live records — with byte-identical decoded values — as the
//! authoritative per-id read path (`all()` / `get()`), under insert + update +
//! delete churn and across a string (variable) column plus fixed columns.
//!
//! The scan drops the per-row tombstone check (it pre-filters the live set via
//! one bulk `Tombstones::live_indices`) and decodes from bulk-loaded column
//! buffers instead of per-row reads — this guards that neither shortcut changes
//! the result versus the trusted decoder.

use forgedb_benchmarks::forgedb_generated::{Database, Post, User};
use forgedb_types::Timestamp;
use std::collections::BTreeMap;
use uuid::Uuid;

fn user(i: u128) -> User {
    User {
        id: Uuid::from_u128(0xA000_0000_0000_0000_0000_0000_0000_0000 + i),
        name: format!("user {i}"),
        email: format!("user{i}@example.com"),
        created_at: Timestamp::from(1_700_000_000 + i as i64),
        posts: (),
    }
}

fn post(i: u128, author: Uuid) -> Post {
    Post {
        id: Uuid::from_u128(0xB000_0000_0000_0000_0000_0000_0000_0000 + i),
        title: format!("title \u{2713} {i}"), // includes a multibyte char
        views: (i as u64) * 7,
        published: i % 2 == 0,
        author,
        created_at: Timestamp::from(1_700_000_500 + i as i64),
        tags: (),
    }
}

/// Ground truth: the live scan fields keyed by id, built from the per-id read
/// path (`all()` returns full records via `get`, which honors tombstones).
fn ground_truth(db: &Database) -> BTreeMap<Uuid, (String, u64, bool, i64)> {
    db.post
        .all()
        .into_iter()
        .map(|p| (p.id, (p.title, p.views, p.published, i64::from(p.created_at))))
        .collect()
}

/// The same map, but decoded through the bulk-buffered `__scan_all()`.
fn from_scan(db: &Database) -> BTreeMap<Uuid, (String, u64, bool, i64)> {
    db.post
        .__scan_all()
        .into_iter()
        .map(|r| (r.id, (r.title, r.views, r.published, i64::from(r.created_at))))
        .collect()
}

/// The projected (views, published) columns keyed by id, from the per-id read
/// path — ground truth for the column-pruned `all_agg()` scan.
fn ground_truth_agg(db: &Database) -> BTreeMap<Uuid, (u64, bool)> {
    db.post
        .all()
        .into_iter()
        .map(|p| (p.id, (p.views, p.published)))
        .collect()
}

/// The same, but decoded through the projected buffered scan `all_agg()` (#113 +
/// #168 column-pruning): it bulk-loads ONLY the `views`/`published` columns and
/// never touches `title`, so this guards that the pruned decode is value-identical
/// to the full read path under the same churn.
fn from_agg_scan(db: &Database) -> BTreeMap<Uuid, (u64, bool)> {
    db.post
        .all_agg()
        .into_iter()
        .map(|r| (r.id, (r.views, r.published)))
        .collect()
}

#[test]
fn buffered_scan_matches_per_row_reads_under_churn() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());

    let author = user(1);
    let author_id = author.id;
    db.user.insert(author).expect("insert user");

    // Insert 500 posts.
    let mut ids = Vec::new();
    for i in 0..500u128 {
        let p = post(i, author_id);
        ids.push(p.id);
        db.post.insert(p).expect("insert post");
    }
    assert_eq!(ground_truth(&db), from_scan(&db), "after inserts");
    assert_eq!(from_scan(&db).len(), 500);
    assert_eq!(ground_truth_agg(&db), from_agg_scan(&db), "projected agg scan after inserts");

    // Update every 3rd post (superseding-version append → the old physical row
    // is superseded; the scan must decode the NEW values, not the stale row).
    for (i, id) in ids.iter().enumerate() {
        if i % 3 == 0 {
            let mut p = post(i as u128, author_id);
            p.title = format!("UPDATED {i}");
            p.views = 1_000_000 + i as u64;
            p.published = !p.published;
            assert!(db.post.update(*id, p).expect("update ok"), "update {i}");
        }
    }
    assert_eq!(ground_truth(&db), from_scan(&db), "after updates");
    assert_eq!(ground_truth_agg(&db), from_agg_scan(&db), "projected agg scan after updates");

    // Delete every 5th post (tombstoned superseding version → id_to_row still
    // points at the tombstoned row; live_indices must exclude it).
    let mut deleted = 0;
    for (i, id) in ids.iter().enumerate() {
        if i % 5 == 0 {
            assert!(db.post.delete(*id), "delete {i}");
            deleted += 1;
        }
    }
    let gt = ground_truth(&db);
    assert_eq!(gt, from_scan(&db), "after deletes");
    assert_eq!(gt.len(), 500 - deleted, "deleted rows must be excluded");
    let gt_agg = ground_truth_agg(&db);
    assert_eq!(gt_agg, from_agg_scan(&db), "projected agg scan after deletes");

    // Reopen (rebuilds id_to_row/tombstones from disk) and re-verify — the scan
    // over a reopened dir (columns faulted from files, non-dense live set after
    // churn) still matches.
    drop(db);
    let db = Database::open_at(dir.path().to_path_buf());
    assert_eq!(gt, from_scan(&db), "after reopen");
    assert_eq!(gt_agg, from_agg_scan(&db), "projected agg scan after reopen");
}

#[test]
fn buffered_scan_empty_model() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open_at(dir.path().to_path_buf());
    assert!(db.post.__scan_all().is_empty(), "empty model scans to []");
}
