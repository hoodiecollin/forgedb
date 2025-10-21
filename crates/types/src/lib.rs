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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_creation() {
        let ts = Timestamp::from_seconds(1234567890);
        assert_eq!(ts.as_seconds(), 1234567890);
    }

    #[test]
    fn test_timestamp_now() {
        let now = Timestamp::now();
        assert!(now.as_seconds() > 0);
        // Should be reasonably recent (after year 2020)
        assert!(now.as_seconds() > 1577836800);
    }

    #[test]
    fn test_timestamp_from_i64() {
        let ts: Timestamp = 1234567890_i64.into();
        assert_eq!(ts.as_seconds(), 1234567890);
    }

    #[test]
    fn test_timestamp_to_i64() {
        let ts = Timestamp::from_seconds(1234567890);
        let seconds: i64 = ts.into();
        assert_eq!(seconds, 1234567890);
    }

    #[test]
    fn test_timestamp_ordering() {
        let ts1 = Timestamp::from_seconds(100);
        let ts2 = Timestamp::from_seconds(200);
        assert!(ts1 < ts2);
        assert!(ts2 > ts1);
        assert_eq!(ts1, Timestamp::from_seconds(100));
    }

    #[test]
    fn test_timestamp_serialization() {
        let ts = Timestamp::from_seconds(1234567890);
        let json = serde_json::to_string(&ts).unwrap();
        assert_eq!(json, "1234567890");

        let deserialized: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ts);
    }

    #[test]
    fn test_value_i32() {
        let val = Value::I32(42);
        assert_eq!(val.type_name(), "i32");
        assert!(val.is_numeric());
        assert!(!val.is_string());
    }

    #[test]
    fn test_value_i64() {
        let val = Value::I64(1234567890);
        assert_eq!(val.type_name(), "i64");
        assert!(val.is_numeric());
    }

    #[test]
    fn test_value_f64() {
        let val = Value::F64(3.14159);
        assert_eq!(val.type_name(), "f64");
        assert!(val.is_numeric());
    }

    #[test]
    fn test_value_bool() {
        let val = Value::Bool(true);
        assert_eq!(val.type_name(), "bool");
        assert!(!val.is_numeric());
    }

    #[test]
    fn test_value_string() {
        let val = Value::String("hello".to_string());
        assert_eq!(val.type_name(), "string");
        assert!(val.is_string());
        assert!(!val.is_numeric());
    }

    #[test]
    fn test_value_uuid() {
        let uuid = Uuid::new_v4();
        let val = Value::Uuid(uuid);
        assert_eq!(val.type_name(), "uuid");
    }

    #[test]
    fn test_value_timestamp() {
        let ts = Timestamp::from_seconds(1234567890);
        let val = Value::Timestamp(ts);
        assert_eq!(val.type_name(), "timestamp");
    }

    #[test]
    fn test_value_from_i32() {
        let val: Value = 42_i32.into();
        assert_eq!(val, Value::I32(42));
    }

    #[test]
    fn test_value_from_i64() {
        let val: Value = 1234567890_i64.into();
        assert_eq!(val, Value::I64(1234567890));
    }

    #[test]
    fn test_value_from_f64() {
        let val: Value = 3.14159_f64.into();
        assert_eq!(val, Value::F64(3.14159));
    }

    #[test]
    fn test_value_from_bool() {
        let val: Value = true.into();
        assert_eq!(val, Value::Bool(true));
    }

    #[test]
    fn test_value_from_string() {
        let val: Value = "hello".to_string().into();
        assert_eq!(val, Value::String("hello".to_string()));
    }

    #[test]
    fn test_value_from_str() {
        let val: Value = "hello".into();
        assert_eq!(val, Value::String("hello".to_string()));
    }

    #[test]
    fn test_value_from_uuid() {
        let uuid = Uuid::new_v4();
        let val: Value = uuid.into();
        assert_eq!(val, Value::Uuid(uuid));
    }

    #[test]
    fn test_value_from_timestamp() {
        let ts = Timestamp::from_seconds(1234567890);
        let val: Value = ts.into();
        assert_eq!(val, Value::Timestamp(ts));
    }

    #[test]
    fn test_value_serialization() {
        // Test i32
        let val = Value::I32(42);
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, val);

        // Test string
        let val = Value::String("hello".to_string());
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, val);

        // Test bool
        let val = Value::Bool(true);
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, val);

        // Test uuid
        let uuid = Uuid::new_v4();
        let val = Value::Uuid(uuid);
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, val);
    }

    #[test]
    fn test_value_equality() {
        assert_eq!(Value::I32(42), Value::I32(42));
        assert_ne!(Value::I32(42), Value::I32(43));
        assert_ne!(Value::I32(42), Value::I64(42));

        let uuid = Uuid::new_v4();
        assert_eq!(Value::Uuid(uuid), Value::Uuid(uuid));

        let ts = Timestamp::from_seconds(1234567890);
        assert_eq!(Value::Timestamp(ts), Value::Timestamp(ts));
    }

    #[test]
    fn test_timestamp_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        let ts1 = Timestamp::from_seconds(100);
        let ts2 = Timestamp::from_seconds(200);
        let ts3 = Timestamp::from_seconds(100);

        set.insert(ts1);
        set.insert(ts2);
        set.insert(ts3); // Duplicate of ts1

        assert_eq!(set.len(), 2); // Only ts1 and ts2
    }
}
