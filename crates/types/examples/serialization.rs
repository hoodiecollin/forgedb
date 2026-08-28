use forgedb_types::{Timestamp, Uuid, Value};

fn main() {
    println!("=== ForgeDB Types - Serialization ===\n");

    println!("--- Timestamp Serialization ---");
    let ts = Timestamp::from_micros(1234567890);
    let json = serde_json::to_string(&ts).unwrap();
    println!("Timestamp as JSON: {}", json);

    let deserialized: Timestamp = serde_json::from_str(&json).unwrap();
    println!("Deserialized: {}", deserialized.as_micros());
    println!("Match: {}\n", ts == deserialized);

    println!("--- Value Serialization ---");

    let val = Value::I32(42);
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("I32 Value:\n{}\n", json);

    let val = Value::String("Hello, ForgeDB!".to_string());
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("String Value:\n{}\n", json);

    let uuid = Uuid::new_v4();
    let val = Value::Uuid(uuid);
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("UUID Value:\n{}\n", json);

    let val = Value::Timestamp(Timestamp::from_micros(1234567890));
    let json = serde_json::to_string_pretty(&val).unwrap();
    println!("Timestamp Value:\n{}\n", json);

    println!("--- Roundtrip Test ---");
    let original = Value::F64(3.14159);
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: Value = serde_json::from_str(&json).unwrap();
    println!("Original: {:?}", original);
    println!("JSON: {}", json);
    println!("Deserialized: {:?}", deserialized);
    println!("Match: {}\n", original == deserialized);

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
