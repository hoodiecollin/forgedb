//! Collections example for forgedb-types
//!
//! This example demonstrates using ForgeDB types in collections like HashMaps and HashSets.

use forgedb_types::{Timestamp, Uuid, Value};
use std::collections::{HashMap, HashSet};

fn main() {
    println!("=== ForgeDB Types - Collections ===\n");

    // Timestamp in HashSet
    println!("--- Timestamps in HashSet ---");
    let mut timestamp_set = HashSet::new();
    
    let ts1 = Timestamp::from_seconds(1000);
    let ts2 = Timestamp::from_seconds(2000);
    let ts3 = Timestamp::from_seconds(1000); // Duplicate of ts1

    timestamp_set.insert(ts1);
    timestamp_set.insert(ts2);
    timestamp_set.insert(ts3);

    println!("Inserted 3 timestamps (2 unique)");
    println!("Set size: {}", timestamp_set.len());
    println!("Contains ts1: {}", timestamp_set.contains(&ts1));
    println!("Contains ts2: {}\n", timestamp_set.contains(&ts2));

    // UUID as HashMap key
    println!("--- UUIDs as HashMap Keys ---");
    let mut user_data = HashMap::new();

    let user1_id = Uuid::new_v4();
    let user2_id = Uuid::new_v4();

    user_data.insert(user1_id, "Alice");
    user_data.insert(user2_id, "Bob");

    println!("Stored {} users", user_data.len());
    println!("User 1: {}", user_data.get(&user1_id).unwrap());
    println!("User 2: {}\n", user_data.get(&user2_id).unwrap());

    // Timestamp ordering
    println!("--- Timestamp Ordering ---");
    let mut timestamps = vec![
        Timestamp::from_seconds(5000),
        Timestamp::from_seconds(1000),
        Timestamp::from_seconds(3000),
        Timestamp::from_seconds(2000),
    ];

    println!("Original: {:?}", timestamps.iter().map(|t| t.as_seconds()).collect::<Vec<_>>());
    timestamps.sort();
    println!("Sorted:   {:?}\n", timestamps.iter().map(|t| t.as_seconds()).collect::<Vec<_>>());

    // Mixed Value collection
    println!("--- Mixed Value Collection ---");
    let mut records = HashMap::new();
    
    records.insert("id", Value::Uuid(Uuid::new_v4()));
    records.insert("age", Value::I32(25));
    records.insert("score", Value::F64(95.5));
    records.insert("active", Value::Bool(true));
    records.insert("name", Value::String("Alice".to_string()));
    records.insert("created_at", Value::Timestamp(Timestamp::now()));

    println!("Record with {} fields:", records.len());
    for (key, value) in &records {
        println!("  {}: {} = {:?}", key, value.type_name(), value);
    }

    // Filter numeric values
    println!("\n--- Filtering Values ---");
    let numeric_count = records.values().filter(|v| v.is_numeric()).count();
    let string_count = records.values().filter(|v| v.is_string()).count();
    println!("Numeric fields: {}", numeric_count);
    println!("String fields: {}", string_count);
}
