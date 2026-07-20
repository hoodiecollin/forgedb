//! #170 durability: with group commit, staged rows use the buffered (no-fsync)
//! WAL append and durability is deferred to the ONE `wal.flush()` + column
//! barrier at `TxHandle::commit`. This guards that a committed transaction's rows
//! survive a reopen (durable despite the buffered staging), and that a
//! rolled-back transaction leaves nothing behind.

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
        // One transaction, 500 buffered (no per-row fsync) staged appends → a
        // single barrier at commit.
        db.transaction(|tx| {
            for i in 0..500u128 {
                tx.create_user(user(i))?;
            }
            Ok(())
        })
        .expect("commit");
        // Visible immediately after commit.
        assert_eq!(db.user.row_count(), 500);
        assert!(db.user.get(Uuid::from_u128(0xC000 + 250)).is_some());
    }
    // Reopen a fresh handle over the same dir: the committed rows are durable
    // (the commit's wal.flush + column barrier made the buffered staged appends
    // durable), so recovery reads all 500 back.
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
        // Commit a baseline of 10.
        db.transaction(|tx| {
            for i in 0..10u128 {
                tx.create_user(user(i))?;
            }
            Ok(())
        })
        .expect("commit baseline");
        // Stage 90 more, then roll back (return Err) — none should persist.
        let _ = db.transaction(|tx| {
            for i in 10..100u128 {
                tx.create_user(user(i))?;
            }
            Err::<(), _>(forgedb_benchmarks::forgedb_generated::TxError::Conflict)
        });
        assert_eq!(db.user.row_count(), 10, "rollback discards staged rows");
    }
    // Reopen: only the committed 10 survive (the rolled-back staged appends, even
    // if their buffered WAL bytes reached the file, are dropped by journal-driven
    // recovery — no commit record).
    {
        let db = Database::open_at(dir.path().to_path_buf());
        assert_eq!(db.user.row_count(), 10, "only committed rows recovered");
    }
}
