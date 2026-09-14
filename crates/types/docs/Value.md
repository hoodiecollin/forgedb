A generic value type that can hold any ForgeDB primitive type

This enum represents all primitive types supported by ForgeDB schemas,
providing a type-safe way to work with heterogeneous data.

# Examples

```rust
use forgedb_types::{Value, Uuid};

let int_val = Value::I32(42);
let uint_val = Value::U64(1_000_000_000_u64);
let str_val = Value::String("hello".to_string());
let uuid_val = Value::Uuid(Uuid::new_v4());

// Serialize to JSON
let json = serde_json::to_string(&int_val).unwrap();
```
