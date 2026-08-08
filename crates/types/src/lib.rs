//! ForgeDB Types
//!
//! Core type definitions for ForgeDB schemas and generated code.
//!
//! # Overview
//!
//! This crate provides type definitions that match ForgeDB's schema language types,
//! enabling type-safe serialization, validation, and storage operations. It is a
//! foundational crate used by generated code and other ForgeDB runtime libraries.
//!
//! # Architecture
//!
//! The crate is designed around two key concepts:
//!
//! - **Primitive Types**: Direct mappings of ForgeDB schema types to Rust types
//! - **Generic Value Enum**: Runtime type information for heterogeneous data
//!
//! All types are designed for zero or minimal overhead with `#[repr(transparent)]`
//! for wrapper types and efficient serialization using Serde derive macros.
//!
//! # Supported Types
//!
//! | ForgeDB Type | Rust Type | Description |
//! |--------------|-----------|-------------|
//! | `u32` | `u32` | 32-bit unsigned integer |
//! | `u64` | `u64` | 64-bit unsigned integer |
//! | `i32` | `i32` | 32-bit signed integer |
//! | `i64` | `i64` | 64-bit signed integer |
//! | `f64` | `f64` | 64-bit floating point |
//! | `bool` | `bool` | Boolean value |
//! | `string` | `String` | UTF-8 encoded text |
//! | `uuid` | [`Uuid`] | Universally unique identifier |
//! | `timestamp` | [`Timestamp`] | Unix timestamp (seconds since epoch) |
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```rust
//! use forgedb_types::{Value, Timestamp, Uuid};
//!
//! // Create a timestamp from the current time
//! let ts = Timestamp::now();
//! println!("Current timestamp: {}", ts.as_seconds());
//!
//! // Work with generic values
//! let values = vec![
//!     Value::I32(42),
//!     Value::String("hello".to_string()),
//!     Value::Uuid(Uuid::new_v4()),
//! ];
//!
//! // Serialize to JSON
//! let json = serde_json::to_string(&values[0]).unwrap();
//! ```
//!
//! ## Type Conversions
//!
//! ```rust
//! use forgedb_types::Value;
//!
//! // Convenient From implementations
//! let val: Value = 42_i32.into();
//! let val: Value = "hello".into();
//!
//! // Type checking
//! if val.is_numeric() {
//!     println!("This is a numeric value");
//! }
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`Timestamp`] - Wrapper around `i64` for Unix timestamps
//! - [`Value`] - Enum that can hold any ForgeDB primitive type
//! - [`Uuid`] - Re-exported from the `uuid` crate
//!
//! ## Key Methods
//!
//! - `Timestamp::now()` - Get current timestamp
//! - `Timestamp::from_seconds(i64)` - Create from seconds since epoch
//! - `Value::type_name()` - Get type name string
//! - `Value::is_numeric()` - Check if numeric type
//!
//! # Related Crates
//!
//! - [`forgedb-storage`](../forgedb_storage) - Uses these types for columnar storage
//! - [`forgedb-parser`](../forgedb_parser) - Parses schemas into these types
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation and usage examples
//! - [uuid crate documentation](https://docs.rs/uuid) for UUID operations

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Re-export uuid for convenience
pub use uuid::Uuid;

/// Unix timestamp representing seconds since the Unix epoch (January 1, 1970 00:00:00 UTC)
///
/// Internally stored as an `i64`, this type provides convenient methods for working
/// with timestamps in ForgeDB schemas.
///
/// # Examples
///
/// ```rust
/// use forgedb_types::Timestamp;
///
/// // Create from current time
/// let now = Timestamp::now();
///
/// // Create from seconds
/// let ts = Timestamp::from_seconds(1234567890);
///
/// // Get underlying value
/// let seconds = ts.as_seconds();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Creates a new timestamp from seconds since Unix epoch
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Timestamp;
    ///
    /// let ts = Timestamp::from_seconds(1234567890);
    /// assert_eq!(ts.as_seconds(), 1234567890);
    /// ```
    #[must_use]
    pub fn from_seconds(seconds: i64) -> Self {
        Timestamp(seconds)
    }

    /// Returns the current timestamp
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Timestamp;
    ///
    /// let now = Timestamp::now();
    /// assert!(now.as_seconds() > 0);
    /// ```
    #[must_use]
    pub fn now() -> Self {
        // Saturate rather than panic if the system clock is set before the Unix
        // epoch: a library constructor should not crash on a misconfigured clock.
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Timestamp(seconds)
    }

    /// Returns the timestamp as seconds since Unix epoch
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Timestamp;
    ///
    /// let ts = Timestamp::from_seconds(1234567890);
    /// assert_eq!(ts.as_seconds(), 1234567890);
    /// ```
    #[must_use]
    pub fn as_seconds(&self) -> i64 {
        self.0
    }

    // ---- #254 STUB SURFACE (deliberately wrong; replaced by the real impl) ----
    #[must_use]
    pub fn from_micros(us: i64) -> Self {
        Timestamp(us)
    }
    #[must_use]
    pub fn as_micros(&self) -> i64 {
        self.0
    }
    #[must_use]
    pub fn to_rfc3339(&self) -> String {
        // Deliberately wrong: a naive seconds-only render with no fraction.
        format!("1970-01-01T00:00:{:02}Z", self.0 % 60)
    }
    pub fn from_rfc3339(_s: &str) -> std::result::Result<Self, TimestampParseError> {
        Err(TimestampParseError)
    }
    #[must_use]
    pub fn floor_to_micros(&self, quantum_us: i64) -> Self {
        // Deliberately wrong: truncation toward zero, the bug res 5 names.
        Timestamp(self.0 / quantum_us * quantum_us)
    }
    #[must_use]
    pub fn is_rfc3339_representable(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampParseError;

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl std::str::FromStr for Timestamp {
    type Err = TimestampParseError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Timestamp::from_rfc3339(s)
    }
}

impl From<i64> for Timestamp {
    fn from(seconds: i64) -> Self {
        Timestamp(seconds)
    }
}

impl From<Timestamp> for i64 {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

/// A generic value type that can hold any ForgeDB primitive type
///
/// This enum represents all primitive types supported by ForgeDB schemas,
/// providing a type-safe way to work with heterogeneous data.
///
/// # Examples
///
/// ```rust
/// use forgedb_types::{Value, Uuid};
///
/// let int_val = Value::I32(42);
/// let uint_val = Value::U64(1_000_000_000_u64);
/// let str_val = Value::String("hello".to_string());
/// let uuid_val = Value::Uuid(Uuid::new_v4());
///
/// // Serialize to JSON
/// let json = serde_json::to_string(&int_val).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    /// 32-bit unsigned integer
    U32(u32),
    /// 64-bit unsigned integer — stored losslessly; `u64` values above `i64::MAX`
    /// cannot be represented by the signed `I64` variant without truncation
    U64(u64),
    /// 32-bit signed integer
    I32(i32),
    /// 64-bit signed integer
    I64(i64),
    /// 64-bit floating point number
    F64(f64),
    /// Boolean value
    Bool(bool),
    /// UTF-8 encoded string
    String(String),
    /// Universally unique identifier
    Uuid(Uuid),
    /// Unix timestamp (seconds since epoch)
    Timestamp(Timestamp),
}

impl Value {
    /// Returns the type name of this value
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Value;
    ///
    /// let val = Value::I32(42);
    /// assert_eq!(val.type_name(), "i32");
    ///
    /// let val = Value::U64(u64::MAX);
    /// assert_eq!(val.type_name(), "u64");
    /// ```
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::U32(_) => "u32",
            Value::U64(_) => "u64",
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::String(_) => "string",
            Value::Uuid(_) => "uuid",
            Value::Timestamp(_) => "timestamp",
        }
    }

    /// Returns true if this value is a numeric type (u32, u64, i32, i64, or f64)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Value;
    ///
    /// assert!(Value::U32(10).is_numeric());
    /// assert!(Value::U64(u64::MAX).is_numeric());
    /// assert!(Value::I32(42).is_numeric());
    /// assert!(Value::F64(3.14).is_numeric());
    /// assert!(!Value::String("hello".to_string()).is_numeric());
    /// ```
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Value::U32(_) | Value::U64(_) | Value::I32(_) | Value::I64(_) | Value::F64(_)
        )
    }

    /// Returns true if this value is a string
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Value;
    ///
    /// assert!(Value::String("hello".to_string()).is_string());
    /// assert!(!Value::I32(42).is_string());
    /// ```
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
}

// Implement From for convenient value construction
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::U32(v)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::U64(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::I32(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<Uuid> for Value {
    fn from(v: Uuid) -> Self {
        Value::Uuid(v)
    }
}

impl From<Timestamp> for Value {
    fn from(v: Timestamp) -> Self {
        Value::Timestamp(v)
    }
}
