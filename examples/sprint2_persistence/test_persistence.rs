// Sprint 2: Persistence Test
//
// Demonstrates that database survives restart (write → close → reopen → read)

use sinkdb_storage::UserStorage;
use std::path::PathBuf;

fn main() {
    println!("=== Sprint 2 Persistence Test ===\n");

    let db_path = PathBuf::from("./test_data/sprint2_db");

    // Clean up any previous test data
    let _ = std::fs::remove_dir_all(&db_path);

    println!("Step 1: Create database and insert users...");
    {
        let mut storage = UserStorage::new(db_path.clone())
            .expect("Failed to create storage");

        let user1 = storage.insert("alice@example.com".to_string())
            .expect("Failed to insert alice");
        let user2 = storage.insert("bob@example.com".to_string())
            .expect("Failed to insert bob");
        let user3 = storage.insert("charlie@example.com".to_string())
            .expect("Failed to insert charlie");

        println!("  ✓ Inserted user {}: {}", user1.id, user1.email);
        println!("  ✓ Inserted user {}: {}", user2.id, user2.email);
        println!("  ✓ Inserted user {}: {}", user3.id, user3.email);

        // storage dropped here - simulates closing the database
    }

    println!("\nStep 2: Reopen database and verify data persisted...");
    {
        let mut storage = UserStorage::new(db_path.clone())
            .expect("Failed to reopen storage");

        println!("  Database reopened. Row count: {}", storage.len());
        assert_eq!(storage.len(), 3, "Expected 3 rows after reopening");

        // Retrieve users by ID
        let user1 = storage.get(1).expect("Failed to get user 1")
            .expect("User 1 not found");
        let user2 = storage.get(2).expect("Failed to get user 2")
            .expect("User 2 not found");
        let user3 = storage.get(3).expect("Failed to get user 3")
            .expect("User 3 not found");

        println!("  ✓ Retrieved user {}: {}", user1.id, user1.email);
        println!("  ✓ Retrieved user {}: {}", user2.id, user2.email);
        println!("  ✓ Retrieved user {}: {}", user3.id, user3.email);

        assert_eq!(user1.email, "alice@example.com");
        assert_eq!(user2.email, "bob@example.com");
        assert_eq!(user3.email, "charlie@example.com");

        // List all users
        let all_users = storage.list_all().expect("Failed to list users");
        println!("\n  All users:");
        for user in &all_users {
            println!("    - {} ({})", user.email, user.id);
        }
        assert_eq!(all_users.len(), 3);
    }

    println!("\nStep 3: Reopen again and insert more data...");
    {
        let mut storage = UserStorage::new(db_path.clone())
            .expect("Failed to reopen storage again");

        let user4 = storage.insert("dave@example.com".to_string())
            .expect("Failed to insert dave");

        println!("  ✓ Inserted user {}: {}", user4.id, user4.email);
        assert_eq!(user4.id, 4);
    }

    println!("\nStep 4: Final verification...");
    {
        let mut storage = UserStorage::new(db_path.clone())
            .expect("Failed to reopen storage for final check");

        println!("  Final row count: {}", storage.len());
        assert_eq!(storage.len(), 4);

        let user4 = storage.get(4).expect("Failed to get user 4")
            .expect("User 4 not found");
        println!("  ✓ User 4 persisted: {} ({})", user4.email, user4.id);

        let all_users = storage.list_all().expect("Failed to list users");
        assert_eq!(all_users.len(), 4);
    }

    println!("\n=== Sprint 2 Persistence Test Complete! ===");
    println!("✓ Database survives restart");
    println!("✓ Data persists across multiple sessions");
    println!("✓ Columnar storage working correctly");

    // Clean up test data
    std::fs::remove_dir_all(&db_path).expect("Failed to clean up test data");
    println!("\nTest data cleaned up.");
}
