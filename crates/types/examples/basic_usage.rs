//! Basic usage example for forgedb-types
//!
//! This example demonstrates creating and working with ForgeDB types.

use forgedb_types::{Timestamp, Uuid, Value};

fn main() {
    println!("=== ForgeDB Types - Basic Usage ===\n");

    // Working with timestamps
    println!("--- Timestamps ---");
    let now = Timestamp::now();
    println!("Current timestamp: {}", now.as_seconds());

    let past = Timestamp::from_seconds(1234567890);
    println!("Past timestamp: {}", past.as_seconds());
    println!("Past < Now: {}\n", past < now);

    // Working with UUIDs
    println!("--- UUIDs ---");
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    println!("UUID 1: {}", id1);
    println!("UUID 2: {}", id2);
    println!("Same? {}\n", id1 == id2);

    // Working with Value enum
    println!("--- Generic Values ---");
    let values = vec![
        Value::I32(42),
        Value::I64(1234567890),
        Value::F64(3.14159),
        Value::Bool(true),
        Value::String("Hello, ForgeDB!".to_string()),
        Value::Uuid(Uuid::new_v4()),
        Value::Timestamp(Timestamp::now()),
    ];

    for (i, val) in values.iter().enumerate() {
        println!(
            "Value {}: type='{}', numeric={}, string={}",
            i + 1,
            val.type_name(),
            val.is_numeric(),
            val.is_string()
        );
    }

    // Type conversions
    println!("\n--- Type Conversions ---");
    let val: Value = 42_i32.into();
    println!("i32 -> Value: {:?}", val);

    let val: Value = "hello".into();
    println!("&str -> Value: {:?}", val);

    let ts: Timestamp = 1234567890_i64.into();
    println!("i64 -> Timestamp: {:?}", ts);

    let seconds: i64 = ts.into();
    println!("Timestamp -> i64: {}", seconds);
}
