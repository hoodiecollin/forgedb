use forgedb_types::*;

#[test]
fn test_timestamp_creation() {
    let ts = Timestamp::from_seconds(1234567890);
    assert_eq!(ts.as_seconds(), 1234567890);
}

#[test]
fn test_timestamp_now() {
    let now = Timestamp::now();
    assert!(now.as_seconds() > 0);
    // Should be reasonably recent (after year 2020)
    assert!(now.as_seconds() > 1577836800);
}

#[test]
fn test_timestamp_from_i64() {
    let ts: Timestamp = 1234567890_i64.into();
    assert_eq!(ts.as_seconds(), 1234567890);
}

#[test]
fn test_timestamp_to_i64() {
    let ts = Timestamp::from_seconds(1234567890);
    let seconds: i64 = ts.into();
    assert_eq!(seconds, 1234567890);
}

#[test]
fn test_timestamp_ordering() {
    let ts1 = Timestamp::from_seconds(100);
    let ts2 = Timestamp::from_seconds(200);
    assert!(ts1 < ts2);
    assert!(ts2 > ts1);
    assert_eq!(ts1, Timestamp::from_seconds(100));
}

#[test]
fn test_timestamp_serialization() {
    let ts = Timestamp::from_seconds(1234567890);
    let json = serde_json::to_string(&ts).unwrap();
    assert_eq!(json, "1234567890");

    let deserialized: Timestamp = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, ts);
}

#[test]
fn test_value_i32() {
    let val = Value::I32(42);
    assert_eq!(val.type_name(), "i32");
    assert!(val.is_numeric());
    assert!(!val.is_string());
}

#[test]
fn test_value_i64() {
    let val = Value::I64(1234567890);
    assert_eq!(val.type_name(), "i64");
    assert!(val.is_numeric());
}

#[test]
fn test_value_f64() {
    let val = Value::F64(3.14159);
    assert_eq!(val.type_name(), "f64");
    assert!(val.is_numeric());
}

#[test]
fn test_value_bool() {
    let val = Value::Bool(true);
    assert_eq!(val.type_name(), "bool");
    assert!(!val.is_numeric());
}

#[test]
fn test_value_string() {
    let val = Value::String("hello".to_string());
    assert_eq!(val.type_name(), "string");
    assert!(val.is_string());
    assert!(!val.is_numeric());
}

#[test]
fn test_value_uuid() {
    let uuid = Uuid::new_v4();
    let val = Value::Uuid(uuid);
    assert_eq!(val.type_name(), "uuid");
}

#[test]
fn test_value_timestamp() {
    let ts = Timestamp::from_seconds(1234567890);
    let val = Value::Timestamp(ts);
    assert_eq!(val.type_name(), "timestamp");
}

#[test]
fn test_value_from_i32() {
    let val: Value = 42_i32.into();
    assert_eq!(val, Value::I32(42));
}

#[test]
fn test_value_from_i64() {
    let val: Value = 1234567890_i64.into();
    assert_eq!(val, Value::I64(1234567890));
}

#[test]
fn test_value_from_f64() {
    let val: Value = 3.14159_f64.into();
    assert_eq!(val, Value::F64(3.14159));
}

#[test]
fn test_value_from_bool() {
    let val: Value = true.into();
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_value_from_string() {
    let val: Value = "hello".to_string().into();
    assert_eq!(val, Value::String("hello".to_string()));
}

#[test]
fn test_value_from_str() {
    let val: Value = "hello".into();
    assert_eq!(val, Value::String("hello".to_string()));
}

#[test]
fn test_value_from_uuid() {
    let uuid = Uuid::new_v4();
    let val: Value = uuid.into();
    assert_eq!(val, Value::Uuid(uuid));
}

#[test]
fn test_value_from_timestamp() {
    let ts = Timestamp::from_seconds(1234567890);
    let val: Value = ts.into();
    assert_eq!(val, Value::Timestamp(ts));
}

#[test]
fn test_value_serialization() {
    // Test i32
    let val = Value::I32(42);
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, val);

    // Test string
    let val = Value::String("hello".to_string());
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, val);

    // Test bool
    let val = Value::Bool(true);
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, val);

    // Test uuid
    let uuid = Uuid::new_v4();
    let val = Value::Uuid(uuid);
    let json = serde_json::to_string(&val).unwrap();
    let deserialized: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, val);
}

#[test]
fn test_value_equality() {
    assert_eq!(Value::I32(42), Value::I32(42));
    assert_ne!(Value::I32(42), Value::I32(43));
    assert_ne!(Value::I32(42), Value::I64(42));

    let uuid = Uuid::new_v4();
    assert_eq!(Value::Uuid(uuid), Value::Uuid(uuid));

    let ts = Timestamp::from_seconds(1234567890);
    assert_eq!(Value::Timestamp(ts), Value::Timestamp(ts));
}

#[test]
fn test_timestamp_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    let ts1 = Timestamp::from_seconds(100);
    let ts2 = Timestamp::from_seconds(200);
    let ts3 = Timestamp::from_seconds(100);

    set.insert(ts1);
    set.insert(ts2);
    set.insert(ts3); // Duplicate of ts1

    assert_eq!(set.len(), 2); // Only ts1 and ts2
}
