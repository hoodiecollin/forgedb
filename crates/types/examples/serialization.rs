//! Serialization example for forgedb-types
//!
//! This example demonstrates JSON serialization and deserialization of ForgeDB types.

use forgedb_types::{Timestamp, Uuid, Value};

fn main() {
    println!("=== ForgeDB Types - Serialization ===\n");

    // Serialize timestamps
    println!("--- Timestamp Serialization ---");
    let ts = Timestamp::from_seconds(1234567890);
    let json = serde_json::to_string(&ts).unwrap();
    println!("Timestamp as JSON: {}", json);

    let deserialized: Timestamp = serde_json::from_str(&json).unwrap();
    println!("Deserialized: {}", deserialized.as_seconds());
    println!("Match: {}\n", ts == deserialized);

    // Serialize Value enum
    println!("--- Value Serialization ---");

    // Integer value
    let val = Value::I32(42);
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("I32 Value:\n{}\n", json);

    // String value
    let val = Value::String("Hello, ForgeDB!".to_string());
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("String Value:\n{}\n", json);

    // UUID value
    let uuid = Uuid::new_v4();
    let val = Value::Uuid(uuid);
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("UUID Value:\n{}\n", json);

    // Timestamp value
    let val = Value::Timestamp(Timestamp::from_seconds(1234567890));
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("Timestamp Value:\n{}\n", json);

    // Roundtrip test
    println!("--- Roundtrip Test ---");
    let original = Value::F64(3.14159);
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: Value = serde_json::from_str(&json).unwrap();
    println!("Original: {:?}", original);
    println!("JSON: {}", json);
    println!("Deserialized: {:?}", deserialized);
    println!("Match: {}\n", original == deserialized);

    // Collection of values
    println!("--- Collection Serialization ---");
    let values = vec![
        Value::I32(42),
        Value::Bool(true),
        Value::String("test".to_string()),
    ];
    let json = serde_json::to_string_pretty(&values).unwrap();
    println!("Values array:\n{}\n", json);

    let deserialized: Vec<Value> = serde_json::from_str(&json).unwrap();
    println!("Deserialized {} values", deserialized.len());
    println!("Match: {}", values == deserialized);
}
