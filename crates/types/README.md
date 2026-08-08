# ForgeDB Types

Core type definitions for ForgeDB schemas and generated code.

## Overview

The `forgedb-types` crate provides type definitions that match ForgeDB's schema language types, enabling type-safe serialization, validation, and storage operations. This is a foundational crate used by generated code and other ForgeDB runtime libraries.

## Features

- **Type-safe primitives** - Wrappers for all ForgeDB schema types
- **Serde support** - Automatic JSON/binary serialization
- **Zero-cost abstractions** - Minimal runtime overhead
- **Comprehensive documentation** - Full API docs with examples
- **Extensive testing** - >80% test coverage

## Supported Types

### Primitive Types

| ForgeDB Type | Rust Type | Description |
|--------------|-----------|-------------|
| `i32` | `i32` | 32-bit signed integer |
| `i64` | `i64` | 64-bit signed integer |
| `f64` | `f64` | 64-bit floating point |
| `bool` | `bool` | Boolean value |
| `string` | `String` | UTF-8 encoded text |
| `uuid` | [`Uuid`](https://docs.rs/uuid) | Universally unique identifier |
| `timestamp` | `Timestamp` | Instant, as microseconds since the Unix epoch |

### Special Types

#### `Timestamp`

A wrapper around `i64` holding **microseconds** since January 1, 1970 00:00:00 UTC.
Microseconds are the one canonical unit, whatever precision a field declares
(`timestamp(s|ms|us)`); the JSON/`Display`/`FromStr` form is RFC 3339.

```rust
use forgedb_types::Timestamp;

// Create from current time
let now = Timestamp::now();

// Create from microseconds
let ts = Timestamp::from_micros(1_775_000_000_123_456);

// Get underlying value
let micros = ts.as_micros();

// The wire form is RFC 3339, in both directions
assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.123456Z");
let back: Timestamp = "2026-03-31T23:33:20.123456Z".parse().unwrap();

// Convert to/from i64 (microseconds)
let ts: Timestamp = 1_775_000_000_123_456_i64.into();
let micros: i64 = ts.into();
```

#### `Value`

A generic enum that can hold any ForgeDB primitive type, useful for working with heterogeneous data.

```rust
use forgedb_types::{Value, Uuid};

// Create values
let int_val = Value::I32(42);
let str_val = Value::String("hello".to_string());
let uuid_val = Value::Uuid(Uuid::new_v4());

// Query type information
assert_eq!(int_val.type_name(), "i32");
assert!(int_val.is_numeric());
assert!(str_val.is_string());

// Serialize to JSON
let json = serde_json::to_string(&int_val).unwrap();
```

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
forgedb-types = "0.1"
```

## Usage Examples

### Basic Types

```rust
use forgedb_types::{Timestamp, Value, Uuid};

// Work with timestamps
let ts = Timestamp::now();
println!("Current timestamp: {}", ts); // RFC 3339

// Create typed values
let values = vec![
    Value::I32(42),
    Value::F64(3.14159),
    Value::Bool(true),
    Value::String("hello world".to_string()),
    Value::Uuid(Uuid::new_v4()),
    Value::Timestamp(Timestamp::now()),
];

// Check types
for val in &values {
    println!("Type: {}", val.type_name());
}
```

### Serialization

```rust
use forgedb_types::{Value, Timestamp};
use serde_json;

// Serialize a value
let val = Value::I32(42);
let json = serde_json::to_string(&val).unwrap();
println!("JSON: {}", json);

// Deserialize back
let deserialized: Value = serde_json::from_str(&json).unwrap();
assert_eq!(val, deserialized);

// Timestamps serialize as an RFC 3339 string, never a bare number
let ts = Timestamp::from_micros(1_775_000_000_123_456);
let json = serde_json::to_string(&ts).unwrap();
assert_eq!(json, "\"2026-03-31T23:33:20.123456Z\"");
```

### Type Conversions

```rust
use forgedb_types::Value;

// Convenient From implementations
let val: Value = 42_i32.into();
let val: Value = 3.14_f64.into();
let val: Value = true.into();
let val: Value = "hello".into();

// Type checking
if val.is_numeric() {
    println!("This is a numeric value");
}
if val.is_string() {
    println!("This is a string value");
}
```

## API Documentation

### Core Types

#### `Timestamp`

- `Timestamp::from_micros(i64) -> Timestamp` - Create from microseconds since epoch
- `Timestamp::now() -> Timestamp` - Get current timestamp
- `ts.as_micros() -> i64` - Get the microseconds value
- `ts.to_rfc3339() -> String` / `Timestamp::from_rfc3339(&str)` - The wire form
- `ts.floor_to_micros(quantum) -> Timestamp` - Floor to a declared precision
- `ts.is_rfc3339_representable() -> bool` - Inside RFC 3339's year range 0000-9999
- Implements: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Serialize`, `Deserialize`

#### `Value`

- `Value::type_name() -> &str` - Get type name string
- `Value::is_numeric() -> bool` - Check if numeric type (i32/i64/f64)
- `Value::is_string() -> bool` - Check if string type
- Implements: `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`
- Variants: `I32(i32)`, `I64(i64)`, `F64(f64)`, `Bool(bool)`, `String(String)`, `Uuid(Uuid)`, `Timestamp(Timestamp)`

#### `Uuid`

Re-exported from the [`uuid`](https://docs.rs/uuid) crate. See its documentation for full API.

## Design Decisions

### Why Wrap Timestamps?

While ForgeDB stores timestamps as plain `i64` values, we provide a `Timestamp` wrapper type for:
- Type safety (prevents mixing timestamps with other integers)
- Convenient constructors (`now()`, `from_micros()`, `from_rfc3339()`)
- Clear intent in APIs
- Future extensibility (e.g., different time units)

The wrapper has zero runtime overhead due to `#[repr(transparent)]` and is serialized as a plain `i64`.

### Why Use `Value` Enum?

The `Value` enum provides:
- Type-safe heterogeneous collections
- Runtime type information
- Generic storage for dynamic data
- JSON/binary serialization with type tags

## Testing

```bash
# Run all tests
cargo test -p forgedb-types

# Run with output
cargo test -p forgedb-types -- --nocapture

# Run doc tests
cargo test -p forgedb-types --doc
```

## Performance

All types are designed for zero or minimal overhead:
- `Timestamp` is `#[repr(transparent)]` over `i64`
- No heap allocations except for `String` and `Value::String`
- `Copy` types where appropriate for stack efficiency
- Serde uses derive macros for optimal code generation

## Dependencies

- [`serde`](https://serde.rs/) - Serialization framework
- [`uuid`](https://docs.rs/uuid) - UUID generation and parsing

Dev dependencies:
- [`serde_json`](https://docs.rs/serde_json) - JSON testing

## Contributing

This crate is part of the ForgeDB project.

## Documentation

For more information about ForgeDB:

- **[ForgeDB Architecture](../../docs/ARCHITECTURE.md)** - System design and component architecture
- **[Public Crates Guide](../../docs/PUBLIC_CRATES.md)** - Complete runtime library documentation
- **[Development Guide](../../docs/DEVELOPMENT.md)** - Development setup and workflow
- **[Contributing Guide](../../docs/CONTRIBUTING.md)** - How to contribute

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT License ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
