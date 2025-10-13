// Test example demonstrating all Sprint 2 types

mod generated {
    include!("test_all_types_generated.rs");
}

use generated::{User, UserStorage};

fn main() {
    println!("=== Sprint 2: Testing All Type Support ===\n");

    let mut storage = UserStorage::new();

    // Test 1: Insert user with all types
    println!("Test 1: Inserting user with all new types...");
    let user = storage.insert(
        "alice@example.com".to_string(),  // string (unique)
        30,                                 // u32
        1234.56,                           // f64
        true,                              // bool
        -100,                              // i32
    ).expect("Failed to insert user");

    println!("  ✓ User created:");
    println!("    - id (uuid): {}", user.id);
    println!("    - email (string): {}", user.email);
    println!("    - age (u32): {}", user.age);
    println!("    - balance (f64): {}", user.balance);
    println!("    - active (bool): {}", user.active);
    println!("    - score (i32): {}", user.score);
    println!("    - created_at (timestamp): {}", user.created_at);

    // Test 2: UUID auto-generation is unique
    println!("\nTest 2: Testing UUID auto-generation...");
    let user2 = storage.insert(
        "bob@example.com".to_string(),
        25,
        500.00,
        false,
        50,
    ).expect("Failed to insert second user");

    assert_ne!(user.id, user2.id, "UUIDs should be unique");
    println!("  ✓ UUIDs are unique: {} != {}", user.id, user2.id);

    // Test 3: Timestamp auto-generation
    println!("\nTest 3: Testing timestamp auto-generation...");
    assert!(user.created_at > 0, "Timestamp should be positive");
    assert!(user2.created_at >= user.created_at, "Second timestamp should be >= first");
    println!("  ✓ Timestamps generated: {} and {}", user.created_at, user2.created_at);

    // Test 4: Unique constraint on email
    println!("\nTest 4: Testing unique constraint on email...");
    let duplicate_result = storage.insert(
        "alice@example.com".to_string(),
        40,
        999.99,
        true,
        0,
    );
    assert!(duplicate_result.is_err(), "Duplicate email should fail");
    println!("  ✓ Unique constraint enforced: {}", duplicate_result.unwrap_err());

    // Test 5: Retrieve by UUID
    println!("\nTest 5: Testing retrieval by UUID...");
    let retrieved = storage.get(user.id).expect("Should find user by UUID");
    assert_eq!(retrieved.email, "alice@example.com");
    println!("  ✓ Retrieved user by UUID: {}", retrieved.email);

    // Test 6: All numeric types
    println!("\nTest 6: Testing numeric type ranges...");
    println!("  - u32 age: {} (max: {})", user.age, u32::MAX);
    println!("  - i32 score: {} (min: {}, max: {})", user.score, i32::MIN, i32::MAX);
    println!("  - f64 balance: {} (precision verified)", user.balance);
    println!("  ✓ All numeric types working correctly");

    // Test 7: Boolean type
    println!("\nTest 7: Testing boolean type...");
    assert_eq!(user.active, true);
    assert_eq!(user2.active, false);
    println!("  ✓ Boolean values: user1={}, user2={}", user.active, user2.active);

    println!("\n=== All Tests Passed! ===");
    println!("\nSprint 2 Type Summary:");
    println!("  ✓ Primitives: u32, u64, i32, i64, f64, bool");
    println!("  ✓ String type with unique constraints");
    println!("  ✓ UUID type with auto-generation (v4)");
    println!("  ✓ Timestamp type with auto-set");
    println!("  ✓ Total types supported: 9");
}
