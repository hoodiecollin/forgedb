Returns the type name of this value

# Examples

```rust
use forgedb_types::Value;

let val = Value::I32(42);
assert_eq!(val.type_name(), "i32");

let val = Value::U64(u64::MAX);
assert_eq!(val.type_name(), "u64");
```
