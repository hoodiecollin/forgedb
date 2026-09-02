use forgedb_benchmarks::forgedb_generated::{Database, Post, User};
use forgedb_types::Timestamp;
use uuid::Uuid;

fn author(db: &mut Database) -> Uuid {
    let u = User {
        id: Uuid::from_u128(0xA1),
        name: "a".into(),
        email: "a@x.com".into(),
        created_at: Timestamp::from(1),
        posts: (),
    };
    let id = u.id;
    db.user.insert(u).unwrap();
    id
}

fn post(i: u128, views: u64, author: Uuid) -> Post {
    Post {
        id: Uuid::from_u128(0xB000 + i),
        title: format!("p{i}"),
        views,
        published: true,
        author,
        created_at: Timestamp::from(100 + i as i64),
        tags: (),
    }
}

fn views_of(rows: &[Post]) -> Vec<u64> {
    rows.iter().map(|p| p.views).collect()
}

#[test]
fn ordered_index_range_and_top_n() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    let a = author(&mut db);

    for (i, v) in [(0u128, 30u64), (1, 10), (2, 50), (3, 20), (4, 40)] {
        db.post.insert(post(i, v, a)).unwrap();
    }

    assert_eq!(
        views_of(&db.post.find_by_views_range(None, None, false, None)),
        vec![10, 20, 30, 40, 50]
    );
    assert_eq!(
        views_of(&db.post.find_by_views_range(None, None, true, None)),
        vec![50, 40, 30, 20, 10]
    );

    assert_eq!(
        views_of(&db.post.find_by_views_range(Some(20), Some(40), false, None)),
        vec![20, 30, 40]
    );

    assert_eq!(
        views_of(&db.post.find_by_views_range(None, None, true, Some(2))),
        vec![50, 40]
    );

    assert_eq!(
        views_of(&db.post.find_by_views_range(Some(25), None, true, Some(2))),
        vec![50, 40]
    );

    assert!(db.post.find_by_views_range(Some(100), None, false, None).is_empty());
    assert!(db.post.find_by_views_range(Some(40), Some(20), false, None).is_empty());
}

#[test]
fn ordered_index_maintained_under_update_delete_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open_at(dir.path().to_path_buf());
    let a = author(&mut db);
    for (i, v) in [(0u128, 30u64), (1, 10), (2, 50), (3, 20), (4, 40)] {
        db.post.insert(post(i, v, a)).unwrap();
    }

    let mut p = post(0, 5, a);
    assert!(db.post.update(Uuid::from_u128(0xB000), p.clone()).unwrap());
    assert_eq!(
        views_of(&db.post.find_by_views_range(None, None, false, None)),
        vec![5, 10, 20, 40, 50]
    );
    assert!(db.post.find_by_views_range(Some(30), Some(30), false, None).is_empty());
    assert_eq!(views_of(&db.post.find_by_views_range(Some(5), Some(5), false, None)), vec![5]);

    assert!(db.post.delete(Uuid::from_u128(0xB002)));
    assert_eq!(
        views_of(&db.post.find_by_views_range(None, None, true, None)),
        vec![40, 20, 10, 5]
    );

    drop(db);
    let db = Database::open_at(dir.path().to_path_buf());
    assert_eq!(
        views_of(&db.post.find_by_views_range(None, None, false, None)),
        vec![5, 10, 20, 40]
    );
    assert_eq!(
        views_of(&db.post.find_by_views_range(Some(10), None, true, Some(2))),
        vec![40, 20]
    );
    let _ = &mut p;
}
