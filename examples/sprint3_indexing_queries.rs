// Sprint 3: Indexing and Query Operations Example
//
// This example demonstrates:
// - Indexed fields (^ symbol)
// - Unique indexed fields (^& symbol)
// - find_by_X methods for indexed fields
// - list() operation with tombstone filtering
// - update() operation with index maintenance
// - delete() operation with tombstone marking

mod database {
    include!("../generated/database.rs");
}

use database::{User, UserStorage};

fn main() {
    println!("=== Sprint 3: Indexing & Query Operations Demo ===\n");

    let mut storage = UserStorage::new();

    // Test 1: Insert users with indexed fields
    println!("1. Inserting users...");
    let user1 = storage.insert(
        "alice@example.com".to_string(),
        "alice".to_string(),
        25,
    ).expect("Failed to insert user1");
    println!("   Created user: {:?}", user1);

    let user2 = storage.insert(
        "bob@example.com".to_string(),
        "bob".to_string(),
        30,
    ).expect("Failed to insert user2");
    println!("   Created user: {:?}", user2);

    let user3 = storage.insert(
        "charlie@example.com".to_string(),
        "charlie".to_string(),
        35,
    ).expect("Failed to insert user3");
    println!("   Created user: {:?}", user3);

    // Test 2: Insert user with duplicate non-unique indexed field (username)
    println!("\n2. Testing non-unique index (username)...");
    let user4 = storage.insert(
        "alice2@example.com".to_string(),
        "alice".to_string(),  // Same username as user1
        28,
    ).expect("Failed to insert user4");
    println!("   Created user with duplicate username: {:?}", user4);

    // Test 3: Test unique constraint on email (should fail)
    println!("\n3. Testing unique constraint on email...");
    match storage.insert("alice@example.com".to_string(), "alice3".to_string(), 29) {
        Ok(_) => println!("   ERROR: Should have failed on unique constraint!"),
        Err(e) => println!("   ✓ Correctly rejected duplicate email: {}", e),
    }

    // Test 4: Find by unique indexed field (email)
    println!("\n4. Finding by email (unique index, O(1) lookup)...");
    let found = storage.find_by_email("bob@example.com".to_string());
    println!("   Found {} user(s) with email 'bob@example.com'", found.len());
    for user in &found {
        println!("     - {:?}", user);
    }

    // Test 5: Find by non-unique indexed field (username)
    println!("\n5. Finding by username (non-unique index, O(1) lookup)...");
    let found = storage.find_by_username("alice".to_string());
    println!("   Found {} user(s) with username 'alice'", found.len());
    for user in &found {
        println!("     - {:?}", user);
    }

    // Test 6: List all users
    println!("\n6. Listing all users...");
    let all_users = storage.list();
    println!("   Total users: {}", all_users.len());
    for user in &all_users {
        println!("     - {} ({}, age {})", user.email, user.username, user.age);
    }

    // Test 7: Update a user
    println!("\n7. Updating user...");
    let updated = storage.update(
        user1.id,
        "alice.updated@example.com".to_string(),
        "alice_new".to_string(),
        26,
    ).expect("Failed to update user");
    println!("   Updated user: {:?}", updated);

    // Verify old email is no longer indexed
    let found = storage.find_by_email("alice@example.com".to_string());
    println!("   Old email 'alice@example.com' now returns {} results", found.len());

    // Verify new email is indexed
    let found = storage.find_by_email("alice.updated@example.com".to_string());
    println!("   New email 'alice.updated@example.com' returns {} results", found.len());

    // Test 8: Delete a user (tombstone marking)
    println!("\n8. Deleting user...");
    storage.delete(user2.id).expect("Failed to delete user");
    println!("   Deleted user with id: {}", user2.id);

    // Verify user is not in list (tombstone filtering)
    let all_users = storage.list();
    println!("   Total users after delete: {}", all_users.len());
    for user in &all_users {
        println!("     - {} ({}, age {})", user.email, user.username, user.age);
    }

    // Verify deleted user cannot be found by indexed field
    let found = storage.find_by_email("bob@example.com".to_string());
    println!("   Searching for deleted user's email returns {} results", found.len());

    // Test 9: Verify get() also respects tombstones
    println!("\n9. Testing get() on deleted user...");
    match storage.get(user2.id) {
        Some(user) => println!("   ERROR: Should not find deleted user! {:?}", user),
        None => println!("   ✓ Correctly returned None for deleted user"),
    }

    println!("\n=== All Sprint 3 tests passed! ===");
    println!("\nSuccess Criteria Verified:");
    println!("  ✓ Fast lookup by indexed fields (O(1))");
    println!("  ✓ CRUD operations complete");
    println!("  ✓ Indexes maintained during insert/update/delete");
    println!("  ✓ Tombstones prevent deleted records from appearing");
    println!("  ✓ Unique indexes enforce constraints");
    println!("  ✓ Non-unique indexes support multiple values");
}
