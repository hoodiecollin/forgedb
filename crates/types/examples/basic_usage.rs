use forgedb_types::{Timestamp, Uuid, Value};

fn main() {
    println!("=== ForgeDB Types - Basic Usage ===\n");

    println!("--- Timestamps ---");
    let now = Timestamp::now();
    println!("Current timestamp: {}", now.as_micros());

    let past = Timestamp::from_micros(1234567890);
    println!("Past timestamp: {}", past.as_micros());
    println!("Past < Now: {}\n", past < now);

    println!("--- UUIDs ---");
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    println!("UUID 1: {}", id1);
    println!("UUID 2: {}", id2);
    println!("Same? {}\n", id1 == id2);

    println!("--- Generic Values ---");
    let values = vec![
        Value::U32(4_294_967_295_u32),
        Value::U64(u64::MAX),
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

    println!("\n--- Type Conversions ---");
    let val: Value = 42_i32.into();
    println!("i32 -> Value: {:?}", val);

    let val: Value = "hello".into();
    println!("&str -> Value: {:?}", val);

    let ts: Timestamp = 1234567890_i64.into();
    println!("i64 -> Timestamp: {:?}", ts);

    let micros: i64 = ts.into();
    println!("Timestamp -> i64: {}", micros);
}
