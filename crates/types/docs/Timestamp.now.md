Returns the current timestamp, in microseconds.

```rust
use forgedb_types::Timestamp;
assert!(Timestamp::now().as_micros() > 1_577_836_800_000_000);
```
