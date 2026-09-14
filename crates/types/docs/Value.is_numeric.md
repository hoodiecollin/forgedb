Returns true if this value is a numeric type (u32, u64, i32, i64, or f64)

# Examples

```rust
use forgedb_types::Value;

assert!(Value::U32(10).is_numeric());
assert!(Value::U64(u64::MAX).is_numeric());
assert!(Value::I32(42).is_numeric());
assert!(Value::F64(3.14).is_numeric());
assert!(!Value::String("hello".to_string()).is_numeric());
```
