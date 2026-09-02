use forgedb_types::{InlineStr, Timestamp, Uuid, Value};

#[test]
fn crate_overview_values_and_timestamp() {
    let ts = Timestamp::now();
    let rendered = format!("{ts}");
    assert!(rendered.contains('T'));

    let values = vec![
        Value::I32(42),
        Value::String("hello".to_string()),
        Value::Uuid(Uuid::new_v4()),
    ];

    let json = serde_json::to_string(&values[0]).unwrap();
    assert!(!json.is_empty());
}

#[test]
fn value_from_conversions_and_type_checks() {
    let val: Value = 42_i32.into();
    assert!(val.is_numeric());

    let val: Value = "hello".into();
    assert!(!val.is_numeric());
}

#[test]
fn timestamp_round_trips_through_rfc3339() {
    let ts = Timestamp::from_micros(1_775_000_000_123_456);
    assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.123456Z");
    assert_eq!(ts.as_micros(), 1_775_000_000_123_456);
    assert_eq!(
        "2026-03-31T23:33:20.123456Z".parse::<Timestamp>().unwrap(),
        ts
    );
}

#[test]
fn timestamp_from_micros_is_identity_on_as_micros() {
    assert_eq!(Timestamp::from_micros(1_234_567_890).as_micros(), 1_234_567_890);
}

#[test]
fn timestamp_now_is_after_2020() {
    assert!(Timestamp::now().as_micros() > 1_577_836_800_000_000);
}

#[test]
fn value_variants_serialize() {
    let int_val = Value::I32(42);
    let uint_val = Value::U64(1_000_000_000_u64);
    let str_val = Value::String("hello".to_string());
    let uuid_val = Value::Uuid(Uuid::new_v4());

    assert!(int_val.is_numeric());
    assert!(uint_val.is_numeric());
    assert!(str_val.is_string());
    assert_eq!(uuid_val.type_name(), "uuid");

    let json = serde_json::to_string(&int_val).unwrap();
    assert!(!json.is_empty());
}

#[test]
fn value_type_name() {
    assert_eq!(Value::I32(42).type_name(), "i32");
    assert_eq!(Value::U64(u64::MAX).type_name(), "u64");
}

#[test]
fn value_is_numeric() {
    assert!(Value::U32(10).is_numeric());
    assert!(Value::U64(u64::MAX).is_numeric());
    assert!(Value::I32(42).is_numeric());
    assert!(Value::F64(3.14).is_numeric());
    assert!(!Value::String("hello".to_string()).is_numeric());
}

#[test]
fn value_is_string() {
    assert!(Value::String("hello".to_string()).is_string());
    assert!(!Value::I32(42).is_string());
}

#[test]
fn inline_str_capacity_is_a_bound_not_a_truncation() {
    let ulid: InlineStr<26> = "01JQZ8Y7X6W5V4T3S2R1Q0P9NM".try_into().unwrap();
    assert_eq!(ulid.len(), 26);
    assert_eq!(ulid.to_string(), "01JQZ8Y7X6W5V4T3S2R1Q0P9NM");

    assert!(InlineStr::<4>::try_from("hello").is_err());
}
