Returns true if this value is a string

# Examples

```rust
use forgedb_types::Value;

assert!(Value::String("hello".to_string()).is_string());
assert!(!Value::I32(42).is_string());
```
