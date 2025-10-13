// This example demonstrates the end-to-end workflow:
// 1. Schema definition
// 2. Code generation
// 3. Using the generated storage

// Include the generated code
// Note: You need to run `cargo run` first to generate this file
#[path = "sprint1_mvp/generated/database.rs"]
mod database;

use database::{User, UserStorage};

fn main() {
    println!("=== SinkDB Sprint 1 MVP Demo ===\n");

    // Create storage
    let mut storage = UserStorage::new();
    println!("✓ Created UserStorage\n");

    // Test 1: Insert users with auto-increment ID
    println!("Test 1: Insert users with auto-increment ID");
    let user1 = storage
        .insert("alice@example.com".to_string())
        .expect("Failed to insert user1");
    println!("  Inserted: {:?}", user1);

    let user2 = storage
        .insert("bob@example.com".to_string())
        .expect("Failed to insert user2");
    println!("  Inserted: {:?}", user2);

    assert_eq!(user1.id, 1);
    assert_eq!(user2.id, 2);
    println!("  ✓ Auto-increment working\n");

    // Test 2: Enforce unique constraint
    println!("Test 2: Enforce unique email constraint");
    match storage.insert("alice@example.com".to_string()) {
        Ok(_) => panic!("Should have failed with unique constraint violation"),
        Err(e) => {
            println!("  Expected error: {}", e);
            println!("  ✓ Unique constraint working\n");
        }
    }

    // Test 3: Retrieve users by ID
    println!("Test 3: Retrieve users by ID");
    let retrieved = storage.get(1).expect("Failed to get user 1");
    println!("  Retrieved user 1: {:?}", retrieved);
    assert_eq!(retrieved.email, "alice@example.com");

    let retrieved = storage.get(2).expect("Failed to get user 2");
    println!("  Retrieved user 2: {:?}", retrieved);
    assert_eq!(retrieved.email, "bob@example.com");

    println!("  ✓ Retrieval working\n");

    // Test 4: Non-existent ID returns None
    println!("Test 4: Non-existent ID returns None");
    let result = storage.get(999);
    assert!(result.is_none());
    println!("  ✓ Get non-existent ID returns None\n");

    println!("=== All Sprint 1 Success Criteria Met! ===");
    println!("✓ Parse simple schema");
    println!("✓ Generate compilable Rust code");
    println!("✓ Insert users with auto-increment ID");
    println!("✓ Enforce unique email constraint");
    println!("✓ Retrieve users by ID");
    println!("✓ All in-memory, no crashes");
}
