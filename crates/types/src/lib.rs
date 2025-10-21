//! # ForgeDB Types
//!
//! Core type definitions for ForgeDB schemas and generated code.
//!
//! This crate provides type definitions that match ForgeDB's schema language types,
//! enabling type-safe serialization, validation, and storage operations.
//!
//! ## Supported Types
//!
//! - **Integers**: `i32`, `i64` - Signed integers
//! - **Floating Point**: `f64` - 64-bit floating point numbers
//! - **Boolean**: `bool` - Boolean values
//! - **UUID**: [`Uuid`] - Universally unique identifiers
//! - **Timestamp**: `i64` - Unix timestamps (seconds since epoch)
//! - **String**: [`String`] - UTF-8 encoded text
//!
//! ## Examples
//!
//! ```rust
//! use forgedb_types::{Value, Timestamp};
//!
//! // Create a timestamp from the current time
//! let ts = Timestamp::now();
//!
//! // Work with generic values
//! let value = Value::I32(42);
//! let json = serde_json::to_string(&value).unwrap();
//! ```

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
    pub fn now() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before Unix epoch");
        Timestamp(duration.as_secs() as i64)
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
    pub fn as_seconds(&self) -> i64 {
        self.0
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
/// let str_val = Value::String("hello".to_string());
/// let uuid_val = Value::Uuid(Uuid::new_v4());
///
/// // Serialize to JSON
/// let json = serde_json::to_string(&int_val).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
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
    /// ```
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::String(_) => "string",
            Value::Uuid(_) => "uuid",
            Value::Timestamp(_) => "timestamp",
        }
    }

    /// Returns true if this value is a numeric type (i32, i64, or f64)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use forgedb_types::Value;
    ///
    /// assert!(Value::I32(42).is_numeric());
    /// assert!(Value::F64(3.14).is_numeric());
    /// assert!(!Value::String("hello".to_string()).is_numeric());
    /// ```
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::I32(_) | Value::I64(_) | Value::F64(_))
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
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
}

// Implement From for convenient value construction
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
