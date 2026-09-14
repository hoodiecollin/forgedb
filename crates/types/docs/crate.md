ForgeDB Types

Core type definitions for ForgeDB schemas and generated code.

# Overview

This crate provides type definitions that match ForgeDB's schema language types,
enabling type-safe serialization, validation, and storage operations. It is a
foundational crate used by generated code and other ForgeDB runtime libraries.

# Architecture

The crate is designed around two key concepts:

- **Primitive Types**: Direct mappings of ForgeDB schema types to Rust types
- **Generic Value Enum**: Runtime type information for heterogeneous data

All types are designed for zero or minimal overhead with `#[repr(transparent)]`
for wrapper types and efficient serialization using Serde derive macros.

# Supported Types

| ForgeDB Type | Rust Type | Description |
|--------------|-----------|-------------|
| `u32` | `u32` | 32-bit unsigned integer |
| `u64` | `u64` | 64-bit unsigned integer |
| `i32` | `i32` | 32-bit signed integer |
| `i64` | `i64` | 64-bit signed integer |
| `f64` | `f64` | 64-bit floating point |
| `bool` | `bool` | Boolean value |
| `string` | `String` | UTF-8 encoded text |
| `string(N)` / `string(N!)` | `String`, or [`InlineStr<N>`](InlineStr) as a **key** | Fixed-slot inline text (#238); a `Copy` key type when the field is an identity or a foreign key to one (#252) |
| `uuid` | [`Uuid`] | Universally unique identifier |
| `timestamp` | [`Timestamp`] | Instant, as **microseconds** since the Unix epoch (#254) |

# Examples

## Basic Usage

```rust
use forgedb_types::{Value, Timestamp, Uuid};

// Create a timestamp from the current time
let ts = Timestamp::now();
println!("Current timestamp: {}", ts);   // RFC 3339

// Work with generic values
let values = vec![
    Value::I32(42),
    Value::String("hello".to_string()),
    Value::Uuid(Uuid::new_v4()),
];

// Serialize to JSON
let json = serde_json::to_string(&values[0]).unwrap();
```

## Type Conversions

```rust
use forgedb_types::Value;

// Convenient From implementations
let val: Value = 42_i32.into();
let val: Value = "hello".into();

// Type checking
if val.is_numeric() {
    println!("This is a numeric value");
}
```

# Public API

## Core Types

- [`Timestamp`] - Wrapper around `i64` microseconds since the Unix epoch
- [`InlineStr`] - A `Copy`, fixed-capacity string; the backing type of a
  `string(N)` identity (#252), and of any foreign key pointing at one (#266)
- [`InlineStrError`] - Why a `&str` did not fit an [`InlineStr`]
- [`Value`] - Enum that can hold any ForgeDB primitive type
- [`Uuid`] - Re-exported from the `uuid` crate

## Key Methods

- `Timestamp::now()` - Get current timestamp
- `Timestamp::from_micros(i64)` / `as_micros()` - The canonical unit
- `Timestamp::to_rfc3339()` / `from_rfc3339(&str)` - The canonical wire form
- `InlineStr::try_from(&str)` / `as_str()` - The key type's bound and its text
- `Value::type_name()` - Get type name string
- `Value::is_numeric()` - Check if numeric type

# Related Crates

- [`forgedb-storage`](../forgedb_storage) - Uses these types for columnar storage
- [`forgedb-parser`](../forgedb_parser) - Parses schemas into these types

# See Also

- [README](./README.md) for detailed documentation and usage examples
- [uuid crate documentation](https://docs.rs/uuid) for UUID operations
