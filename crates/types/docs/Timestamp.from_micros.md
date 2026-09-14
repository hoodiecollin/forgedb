Creates a timestamp from microseconds since the Unix epoch.

```rust
use forgedb_types::Timestamp;
assert_eq!(Timestamp::from_micros(1_234_567_890).as_micros(), 1_234_567_890);
```
