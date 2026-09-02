use forgedb_benchmarks::forgedb_generated::{Database, User};
use forgedb_types::Timestamp;
use uuid::Uuid;

fn user(i: u128) -> User {
    User {
        id: Uuid::from_u128(0xC000 + i),
        name: format!("u{i}"),
        email: format!("u{i}@x.com"),
        created_at: Timestamp::from(1_000 + i as i64),
        posts: (),
    }
}

#[test]
fn group_commit_rows_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut db = Database::open_at(dir.path().to_path_buf());
        db.transaction(|tx| {
            for i in 0..500u128 {
                tx.create_user(user(i))?;
            }
            Ok(())
        })
        .expect("commit");
        assert_eq!(db.user.row_count(), 500);
        assert!(db.user.get(Uuid::from_u128(0xC000 + 250)).is_some());
    }
    {
        let db = Database::open_at(dir.path().to_path_buf());
        assert_eq!(db.user.row_count(), 500, "committed rows durable across reopen");
        for i in 0..500u128 {
            let u = db.user.get(Uuid::from_u128(0xC000 + i)).expect("row present");
            assert_eq!(u.email, format!("u{i}@x.com"));
        }
    }
}

#[test]
fn rolled_back_transaction_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut db = Database::open_at(dir.path().to_path_buf());
        db.transaction(|tx| {
            for i in 0..10u128 {
                tx.create_user(user(i))?;
            }
            Ok(())
        })
        .expect("commit baseline");
        let _ = db.transaction(|tx| {
            for i in 10..100u128 {
                tx.create_user(user(i))?;
            }
            Err::<(), _>(forgedb_benchmarks::forgedb_generated::TxError::Conflict)
        });
        assert_eq!(db.user.row_count(), 10, "rollback discards staged rows");
    }
    {
        let db = Database::open_at(dir.path().to_path_buf());
        assert_eq!(db.user.row_count(), 10, "only committed rows recovered");
    }
}
