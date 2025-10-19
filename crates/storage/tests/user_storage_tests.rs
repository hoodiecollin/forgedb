use forgedb_storage::*;
use std::fs;

#[test]
fn test_user_storage_insert_and_get() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_user_storage");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    // Insert users
    let user1 = storage.insert("alice@example.com".to_string()).unwrap();
    let user2 = storage.insert("bob@example.com".to_string()).unwrap();

    assert_eq!(user1.id, 1);
    assert_eq!(user1.email, "alice@example.com");
    assert_eq!(user2.id, 2);
    assert_eq!(user2.email, "bob@example.com");

    // Get users
    let retrieved1 = storage.get(1).unwrap().unwrap();
    let retrieved2 = storage.get(2).unwrap().unwrap();

    assert_eq!(retrieved1, user1);
    assert_eq!(retrieved2, user2);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_unique_constraint() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_unique");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    storage.insert("alice@example.com".to_string()).unwrap();

    // Try to insert duplicate email
    let result = storage.insert("alice@example.com".to_string());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unique constraint violation"));

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_persistence() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_persistence");
    let _ = fs::remove_dir_all(&temp_dir);

    // Create and insert data
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();
        storage.insert("alice@example.com".to_string()).unwrap();
        storage.insert("bob@example.com".to_string()).unwrap();
        storage.insert("charlie@example.com".to_string()).unwrap();
    }

    // Reopen and verify data persisted
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

        assert_eq!(storage.len(), 3);

        let user1 = storage.get(1).unwrap().unwrap();
        let user2 = storage.get(2).unwrap().unwrap();
        let user3 = storage.get(3).unwrap().unwrap();

        assert_eq!(user1.email, "alice@example.com");
        assert_eq!(user2.email, "bob@example.com");
        assert_eq!(user3.email, "charlie@example.com");

        // Verify list_all
        let all_users = storage.list_all().unwrap();
        assert_eq!(all_users.len(), 3);

        // Insert more data after reopening
        let user4 = storage.insert("dave@example.com".to_string()).unwrap();
        assert_eq!(user4.id, 4);
    }

    // Reopen again to verify new data persisted
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();
        assert_eq!(storage.len(), 4);

        let user4 = storage.get(4).unwrap().unwrap();
        assert_eq!(user4.email, "dave@example.com");
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_list_all() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_list_all");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    storage.insert("alice@example.com".to_string()).unwrap();
    storage.insert("bob@example.com".to_string()).unwrap();
    storage.insert("charlie@example.com".to_string()).unwrap();

    let users = storage.list_all().unwrap();
    assert_eq!(users.len(), 3);

    assert_eq!(users[0].email, "alice@example.com");
    assert_eq!(users[1].email, "bob@example.com");
    assert_eq!(users[2].email, "charlie@example.com");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_get_nonexistent() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_nonexistent");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    // Get from empty database
    let result = storage.get(1).unwrap();
    assert!(result.is_none());

    // Insert and try to get non-existent ID
    storage.insert("alice@example.com".to_string()).unwrap();
    let result = storage.get(999).unwrap();
    assert!(result.is_none());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_empty_database() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_empty_db");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    // Empty database operations
    assert_eq!(storage.len(), 0);
    let all_users = storage.list_all().unwrap();
    assert_eq!(all_users.len(), 0);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_large_dataset() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_large");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    // Insert 1000 users
    for i in 0..1000 {
        let email = format!("user{}@example.com", i);
        storage.insert(email).unwrap();
    }

    assert_eq!(storage.len(), 1000);

    // Verify random access works
    let user = storage.get(1).unwrap().unwrap();
    assert_eq!(user.email, "user0@example.com");

    let user = storage.get(500).unwrap().unwrap();
    assert_eq!(user.email, "user499@example.com");

    let user = storage.get(1000).unwrap().unwrap();
    assert_eq!(user.email, "user999@example.com");

    // Verify list_all works
    let all_users = storage.list_all().unwrap();
    assert_eq!(all_users.len(), 1000);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_unique_constraint_after_reopen() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_unique_reopen");
    let _ = fs::remove_dir_all(&temp_dir);

    // Insert user and close
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();
        storage.insert("alice@example.com".to_string()).unwrap();
    }

    // Reopen and verify unique constraint still enforced
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();
        let result = storage.insert("alice@example.com".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unique constraint violation"));

        // But different email should work
        let user = storage.insert("bob@example.com".to_string()).unwrap();
        assert_eq!(user.id, 2);
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_id_continuity_after_reopen() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_id_continuity");
    let _ = fs::remove_dir_all(&temp_dir);

    // Insert 3 users
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();
        storage.insert("user1@example.com".to_string()).unwrap();
        storage.insert("user2@example.com".to_string()).unwrap();
        storage.insert("user3@example.com".to_string()).unwrap();
    }

    // Reopen and verify next ID is 4
    {
        let mut storage = UserStorage::new(temp_dir.clone()).unwrap();
        let user = storage.insert("user4@example.com".to_string()).unwrap();
        assert_eq!(user.id, 4);
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_empty_email() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_empty_email");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    // Empty string should work (validation is for Sprint 2b)
    let user = storage.insert("".to_string()).unwrap();
    assert_eq!(user.email, "");

    let retrieved = storage.get(user.id).unwrap().unwrap();
    assert_eq!(retrieved.email, "");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_user_storage_long_email() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_long_email");
    let _ = fs::remove_dir_all(&temp_dir);

    let mut storage = UserStorage::new(temp_dir.clone()).unwrap();

    // Very long email (1KB)
    let long_email = format!("{}@example.com", "x".repeat(1000));
    let user = storage.insert(long_email.clone()).unwrap();

    let retrieved = storage.get(user.id).unwrap().unwrap();
    assert_eq!(retrieved.email, long_email);

    fs::remove_dir_all(&temp_dir).unwrap();
}
