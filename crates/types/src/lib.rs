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
//! | `string(N)` / `string(N!)` | `String`, or [`InlineStr<N>`](InlineStr) as a **key** | Fixed-slot inline text (#238); a `Copy` key type when the field is an identity or a foreign key to one (#252) |
//! | `uuid` | [`Uuid`] | Universally unique identifier |
//! | `timestamp` | [`Timestamp`] | Instant, as **microseconds** since the Unix epoch (#254) |
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
//! println!("Current timestamp: {}", ts);   // RFC 3339
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
//! - [`Timestamp`] - Wrapper around `i64` microseconds since the Unix epoch
//! - [`InlineStr`] - A `Copy`, fixed-capacity string; the backing type of a
//!   `string(N)` identity (#252), and of any foreign key pointing at one (#266)
//! - [`InlineStrError`] - Why a `&str` did not fit an [`InlineStr`]
//! - [`Value`] - Enum that can hold any ForgeDB primitive type
//! - [`Uuid`] - Re-exported from the `uuid` crate
//!
//! ## Key Methods
//!
//! - `Timestamp::now()` - Get current timestamp
//! - `Timestamp::from_micros(i64)` / `as_micros()` - The canonical unit
//! - `Timestamp::to_rfc3339()` / `from_rfc3339(&str)` - The canonical wire form
//! - `InlineStr::try_from(&str)` / `as_str()` - The key type's bound and its text
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

/// Microseconds elapsed since the Unix epoch (January 1, 1970 00:00:00 UTC).
///
/// # The unit is microseconds (#254)
///
/// Before v0.4.0 this held whole **seconds**. It now holds **microseconds**, and
/// microseconds are the one canonical on-disk unit regardless of a field's
/// declared `timestamp(s|ms|us)` precision — a `timestamp(ms)` field simply
/// stores millisecond-aligned microsecond values. Storing each field in its own
/// unit would make a `Timestamp` value unit-ambiguous at runtime, which is only
/// resolvable with a fatter value (a layout change) or a `Timestamp<const U>`
/// (generated signature churn everywhere).
///
/// `i64` microseconds spans ±292,277 years, which is why microseconds rather
/// than nanoseconds are the floor: nanoseconds would cap the type at 1678–2262,
/// putting a birth date or a long-dated bond maturity out of range.
///
/// # The wire form is RFC 3339, not a number
///
/// [`Display`], [`FromStr`](std::str::FromStr) and serde all speak RFC 3339 —
/// one rendering everywhere, so the create handler's `id.to_string()` and a REST
/// path parameter's `parse()` agree with no generated codec. Output is fixed
/// (**exactly 6 fractional digits, always `Z`**, including for a `timestamp(s)`
/// field: `Display` is on the type and cannot see a field's precision); input is
/// lenient (fewer or more fractional digits, lowercase `t`/`z`, any numeric
/// offset).
///
/// A number would have been a *silent* break — a client reading seconds would
/// start reading microseconds and display 1970 forever. A client expecting a
/// number and receiving a string fails immediately and visibly.
///
/// # Range
///
/// The rendering has no failure mode, so the **type's** range is RFC 3339's year
/// range 0000–9999 ([`is_rfc3339_representable`](Timestamp::is_rfc3339_representable)),
/// enforced by the generated write path rather than here. Values outside it
/// still render and re-parse losslessly (with an extended year), so no data is
/// trapped; they are simply refused at the boundary.
///
/// # Examples
///
/// ```rust
/// use forgedb_types::Timestamp;
///
/// let ts = Timestamp::from_micros(1_775_000_000_123_456);
/// assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.123456Z");
/// assert_eq!(ts.as_micros(), 1_775_000_000_123_456);
/// assert_eq!("2026-03-31T23:33:20.123456Z".parse::<Timestamp>().unwrap(), ts);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

/// Microseconds in one day — the quantum the civil-date split works in.
const US_PER_DAY: i64 = 86_400_000_000;
/// Microseconds in one second.
const US_PER_SEC: i64 = 1_000_000;

/// `0000-01-01T00:00:00Z`, the first instant RFC 3339 can render.
const RFC3339_MIN_US: i64 = -62_167_219_200_000_000;
/// `9999-12-31T23:59:59.999999Z`, the last one.
const RFC3339_MAX_US: i64 = 253_402_300_799_999_999;

impl Timestamp {
    /// Creates a timestamp from microseconds since the Unix epoch.
    ///
    /// ```rust
    /// use forgedb_types::Timestamp;
    /// assert_eq!(Timestamp::from_micros(1_234_567_890).as_micros(), 1_234_567_890);
    /// ```
    #[must_use]
    pub fn from_micros(micros: i64) -> Self {
        Timestamp(micros)
    }

    /// Returns the timestamp as microseconds since the Unix epoch.
    #[must_use]
    pub fn as_micros(&self) -> i64 {
        self.0
    }

    /// Returns the current timestamp, in microseconds.
    ///
    /// ```rust
    /// use forgedb_types::Timestamp;
    /// assert!(Timestamp::now().as_micros() > 1_577_836_800_000_000);
    /// ```
    #[must_use]
    pub fn now() -> Self {
        // Saturate rather than panic if the system clock is set before the Unix
        // epoch: a library constructor should not crash on a misconfigured clock.
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Timestamp(micros)
    }

    /// Renders the instant as RFC 3339: exactly 6 fractional digits, always `Z`.
    ///
    /// Total by construction — this is what [`Display`] calls, and `Display`
    /// cannot fail. A value outside RFC 3339's year range renders with an
    /// extended year (`+10000-…` / `-0001-…`) so the round-trip stays lossless;
    /// such a value is refused at the write boundary instead, via
    /// [`is_rfc3339_representable`](Self::is_rfc3339_representable).
    #[must_use]
    pub fn to_rfc3339(&self) -> String {
        // Floor division, not truncation: for a pre-epoch value the day index
        // must round *down* and the remainder stay non-negative, or the clock
        // fields come out negative.
        let days = self.0.div_euclid(US_PER_DAY);
        let rem = self.0.rem_euclid(US_PER_DAY);
        let (y, m, d) = civil_from_days(days);
        let (h, min, s, us) = (
            rem / 3_600_000_000,
            (rem / 60_000_000) % 60,
            (rem / US_PER_SEC) % 60,
            rem % US_PER_SEC,
        );
        let year = if (0..=9999).contains(&y) {
            format!("{y:04}")
        } else if y < 0 {
            format!("-{:04}", -y)
        } else {
            format!("+{y}")
        };
        format!("{year}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{us:06}Z")
    }

    /// Parses an RFC 3339 instant. Lenient about everything the rendering fixes:
    /// any number of fractional digits (sub-microsecond digits are **truncated**,
    /// since the stored unit is the finest thing the value can mean), a lowercase
    /// `t`/`z` separator, and any numeric offset (`±HH:MM` / `±HHMM` / `±HH`).
    ///
    /// An offset is applied, not recorded: the stored value is an instant.
    pub fn from_rfc3339(s: &str) -> std::result::Result<Self, TimestampParseError> {
        parse_rfc3339(s).map(Timestamp).ok_or(TimestampParseError)
    }

    /// Floors the value to a multiple of `quantum_us` (res 5): a user-supplied
    /// value finer than a field's declared precision is *recorded* coarsely
    /// rather than refused, because rejecting a valid measurement for being too
    /// precise only exports the flooring to every client.
    ///
    /// **Floor, never truncate toward zero.** Truncation rounds a pre-epoch value
    /// *forward* in time (`-0.5s` → `0s`), which breaks monotonicity across 1970
    /// — two instants an hour apart could quantize to the same value from
    /// opposite sides, or swap order.
    ///
    /// A `quantum_us` of `0` or less is the identity (there is nothing to floor
    /// to), so this never divides by zero on a caller's behalf.
    #[must_use]
    pub fn floor_to_micros(&self, quantum_us: i64) -> Self {
        if quantum_us <= 1 {
            return *self;
        }
        Timestamp(self.0.div_euclid(quantum_us) * quantum_us)
    }

    /// Whether the value renders inside RFC 3339's year range (0000–9999).
    ///
    /// `i64` microseconds spans ±292,277 years, so the wire form is narrower than
    /// the storage type. Since [`Display`] cannot fail, the narrowing is enforced
    /// as a validity constraint at the write boundary (a 422) rather than as a
    /// rendering error — and this predicate is what the generated write path
    /// checks.
    #[must_use]
    pub fn is_rfc3339_representable(&self) -> bool {
        (RFC3339_MIN_US..=RFC3339_MAX_US).contains(&self.0)
    }
}

/// Why an RFC 3339 string could not be read as a [`Timestamp`].
///
/// Deliberately opaque: this reaches users through a 400/422 on a REST path
/// segment or query parameter, where the useful information is "that is not an
/// RFC 3339 instant", not which of fourteen sub-fields was malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimestampParseError;

impl std::fmt::Display for TimestampParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not an RFC 3339 timestamp (expected e.g. 2026-03-31T23:33:20.123456Z)")
    }
}

impl std::error::Error for TimestampParseError {}

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

impl Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> std::result::Result<S::Ok, S::Error> {
        ser.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> std::result::Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(de)?;
        Timestamp::from_rfc3339(&s).map_err(serde::de::Error::custom)
    }
}

// ---- RFC 3339 <-> micros, hand-rolled ---------------------------------------
//
// `forgedb-types` is linked by every generated project and must build for
// `wasm32-unknown-unknown`, so its dependency list is deliberately two crates
// long. Pulling `chrono` in for two functions would add a large transitive
// surface and a wasm feature-flag question for no gain: the civil-date
// conversion below is Howard Hinnant's, exact over the whole `i64` micros range,
// and about sixty lines.

/// Days since 1970-01-01 for a proleptic-Gregorian civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// The proleptic-Gregorian civil date for a day index since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Days in `m` of year `y`, for the calendar-validity check. A parse that
/// skipped this would silently accept `2026-02-30` and normalize it to March 2 —
/// a wrong date accepted quietly, which is the failure class this issue is about.
fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// `Some(micros)` for a well-formed RFC 3339 instant, `None` otherwise. Returns
/// an `Option` rather than panicking anywhere: every caller is a REST boundary.
fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let mut i = 0usize;

    // Year: an optional sign then at least four digits (exactly four unless the
    // year is signed — the extended form only ever comes back from our own
    // rendering of an out-of-range value).
    let neg_year = match b.first() {
        Some(b'-') => {
            i = 1;
            true
        }
        Some(b'+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let year_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let year_digits = i - year_start;
    if year_digits < 4 || (year_digits > 4 && year_start == 0) {
        return None;
    }
    let year: i64 = s[year_start..i].parse().ok()?;
    let year = if neg_year { -year } else { year };

    let month = take_fixed(b, &mut i, b'-', 2)?;
    let day = take_fixed(b, &mut i, b'-', 2)?;

    // Date/time separator: `T`, `t`, or a space (RFC 3339 §5.6's NOTE).
    if i >= b.len() || !matches!(b[i], b'T' | b't' | b' ') {
        return None;
    }
    i += 1;

    let hour = take_digits(b, &mut i, 2)?;
    let minute = take_fixed(b, &mut i, b':', 2)?;
    let second = take_fixed(b, &mut i, b':', 2)?;

    // Calendar and clock validity. A leap second (`:60`) is refused rather than
    // clamped: there is no microsecond value for it, and silently mapping it to
    // `:59.999999` would move the instant.
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    // Fraction: at least one digit if the `.` is present. Digits past the sixth
    // are truncated (the stored unit is the finest meaning available).
    let mut micros_of_second = 0i64;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
        let digits = &s[start..(start + 6).min(i)];
        micros_of_second = digits.parse::<i64>().ok()? * 10i64.pow(6 - digits.len() as u32);
    }

    // Offset: `Z`/`z`, or a signed `HH`, `HHMM`, `HH:MM`. Required — a bare local
    // time is not an instant, so it is not a Timestamp.
    let offset_seconds = match b.get(i) {
        Some(b'Z') | Some(b'z') => {
            i += 1;
            0
        }
        Some(c @ (b'+' | b'-')) => {
            let sign = if *c == b'-' { -1 } else { 1 };
            i += 1;
            let oh = take_digits(b, &mut i, 2)?;
            let om = if i < b.len() {
                if b[i] == b':' {
                    i += 1;
                }
                if i < b.len() && b[i].is_ascii_digit() {
                    take_digits(b, &mut i, 2)?
                } else {
                    return None;
                }
            } else {
                0
            };
            if oh > 23 || om > 59 {
                return None;
            }
            sign * (oh * 3600 + om * 60)
        }
        _ => return None,
    };
    // Trailing anything (including a space) is a malformed instant, not a
    // forgivable one — a lenient tail is how `"2026-…Z garbage"` gets accepted.
    if i != b.len() {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let secs_of_day = hour * 3600 + minute * 60 + second - offset_seconds;
    days.checked_mul(US_PER_DAY)?
        .checked_add(secs_of_day.checked_mul(US_PER_SEC)?)?
        .checked_add(micros_of_second)
}

/// Consume exactly `n` ASCII digits at `i`, advancing it.
fn take_digits(b: &[u8], i: &mut usize, n: usize) -> Option<i64> {
    if *i + n > b.len() {
        return None;
    }
    let mut v: i64 = 0;
    for k in 0..n {
        let c = b[*i + k];
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + i64::from(c - b'0');
    }
    *i += n;
    Some(v)
}

/// Consume a fixed separator then exactly `n` digits.
fn take_fixed(b: &[u8], i: &mut usize, sep: u8, n: usize) -> Option<i64> {
    if b.get(*i) != Some(&sep) {
        return None;
    }
    *i += 1;
    take_digits(b, i, n)
}

impl From<i64> for Timestamp {
    /// Microseconds since the epoch, matching [`Timestamp::from_micros`].
    fn from(micros: i64) -> Self {
        Timestamp(micros)
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
    /// An instant (microseconds since the Unix epoch); JSON form is RFC 3339
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


// ---- InlineStr: the `Copy` fixed-capacity string key (#252) ------------------

/// A `Copy`, fixed-capacity UTF-8 string holding at most `BYTES` bytes.
///
/// # Why it exists
///
/// The generated code passes a model's identity **by value**, repeatedly —
/// `get(id)`, `delete(id)`, relation resolution, index-key construction, the
/// live-query delta enum. That is sound for `Uuid`/`u32`/`u64`/[`Timestamp`]
/// because they are `Copy`; `String` is not, so `id: string` produced 31
/// move/borrow errors across a single model. Making the key type `Copy` is far
/// smaller than making every id-consuming path stop copying, and it is what
/// `string(N)` / `string(N!)` identities are built on (#252).
///
/// # Class-1 substrate
///
/// It knows nothing about schemas, identities or URLs. In particular the
/// URL-path-segment alphabet a *string identity's value* must obey (#252 res 4)
/// is **not** enforced here — that rule applies because a field is an identity,
/// which is schema knowledge, so the check is emitted into the generated write
/// path (res 7). What this type enforces is only the capacity bound.
///
/// # Parameterized by bytes, declared in characters
///
/// `BYTES` is a byte capacity, not a character count: the substrate cannot
/// compute `[u8; CHARS * 4]` without the unstable `generic_const_exprs`, so the
/// generator emits the constant. For an identity that mapping is the identity
/// function — `string(26)` and `string(26!)` both give `InlineStr<26>`, because
/// the column is one byte per character (#238) and `@utf8` on an identity is a
/// validation error (#252 res 3). The `4N` form survives for non-identity
/// columns, which is why the parameter stays in bytes.
///
/// # Nothing here is derived except `Clone, Copy`
///
/// | impl | why it is hand-written |
/// |---|---|
/// | `PartialEq`/`Eq`/`PartialOrd`/`Ord`/`Hash` | defined on [`Self::as_str`]; the bytes past `len` are not part of the value (#252 res 8) |
/// | `Serialize`/`Deserialize` | a JSON **string**. serde's *derive* has per-length array impls that stop at 32 — the #243 defect (`char(N)` above 32 could not be indexed). Deriving here would compile for `InlineStr<26>` and fail for `InlineStr<64>` |
/// | `Debug` | a derive would print `BYTES` integers into every log line and panic message |
///
/// # Examples
///
/// ```rust
/// use forgedb_types::InlineStr;
///
/// let ulid: InlineStr<26> = "01JQZ8Y7X6W5V4T3S2R1Q0P9NM".try_into().unwrap();
/// assert_eq!(ulid.len(), 26);
/// assert_eq!(ulid.to_string(), "01JQZ8Y7X6W5V4T3S2R1Q0P9NM");
///
/// // The capacity is a bound, never a truncation.
/// assert!(InlineStr::<4>::try_from("hello").is_err());
/// ```
#[derive(Clone, Copy)]
pub struct InlineStr<const BYTES: usize> {
    buf: [u8; BYTES],
    /// Bytes of `buf` that are in use.
    ///
    /// `u16` rather than `u8` because `InlineStr` is not identity-only: #256 uses
    /// it for bounded string *index keys*, and a non-identity `string(255) @utf8`
    /// column reaches `BYTES = 1020`. For an identity `BYTES <= 255` always.
    len: u16,
}

/// Why a `&str` did not fit an [`InlineStr`].
///
/// Carries both numbers so the generated 422 can name the bound without
/// re-deriving it from the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineStrError {
    /// The length of the offered value, in bytes.
    pub got_bytes: usize,
    /// The capacity of the target `InlineStr`, in bytes.
    pub capacity: usize,
}

impl std::fmt::Display for InlineStrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "value is {} bytes, but the column holds at most {}",
            self.got_bytes, self.capacity
        )
    }
}

impl std::error::Error for InlineStrError {}

impl<const BYTES: usize> InlineStr<BYTES> {
    /// The byte capacity, as a constant.
    pub const CAPACITY: usize = BYTES;

    /// Tripped at monomorphization if a width could not record its own length.
    /// A `u16` `len` caps the type at 65535 bytes, well above the 1020 that
    /// `string(255) @utf8` reaches — this exists so the cap is a compile error
    /// rather than a silent truncating `as u16`.
    const _LEN_FITS: () = assert!(
        BYTES <= u16::MAX as usize,
        "InlineStr records its length in a u16, so BYTES must be at most 65535",
    );

    /// The text, borrowed.
    ///
    /// Infallible: UTF-8 is checked at construction, and construction is the only
    /// writer of `buf`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `buf` is private and every constructor copies a whole `&str`
        // into it — the over-capacity case is *rejected* rather than truncated,
        // so a partial UTF-8 sequence can never be stored. `buf[..len]` is
        // therefore always a complete, valid UTF-8 sequence.
        unsafe { std::str::from_utf8_unchecked(&self.buf[..self.len as usize]) }
    }

    /// The used bytes, borrowed. Excludes the unused tail.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }

    /// Length in **bytes** (matching `str::len`), not in characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the value is the empty string.
    ///
    /// The empty string is a legal `InlineStr` — it is the [`Default`], which the
    /// generated id field's `#[serde(default)]` needs — and is rejected as a
    /// *key* at write time (#252 res 5), not here.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<const BYTES: usize> Default for InlineStr<BYTES> {
    /// The empty string.
    ///
    /// Every generated model struct carries `#[serde(default)]` on its id field
    /// so a create body may omit it. That is safe against res 5 because the empty
    /// key is rejected at *write*: the default exists for the
    /// deserialize-then-overwrite path to land in.
    fn default() -> Self {
        Self {
            buf: [0u8; BYTES],
            len: 0,
        }
    }
}

impl<const BYTES: usize> TryFrom<&str> for InlineStr<BYTES> {
    type Error = InlineStrError;

    /// Copy `s` in whole, or fail.
    ///
    /// **Never truncates.** A silently shortened key writes a row under an id the
    /// caller never asked for, and the caller's own copy of the id then resolves
    /// to nothing.
    ///
    /// The unused tail is zeroed. Nothing depends on it — [`Self::as_str`] slices
    /// to `len`, and comparison is defined on the text — but it makes two equal
    /// values byte-identical, which is what lets an inline column file stay
    /// byte-reproducible for the same logical content (#238).
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let () = Self::_LEN_FITS;
        let b = s.as_bytes();
        if b.len() > BYTES {
            return Err(InlineStrError {
                got_bytes: b.len(),
                capacity: BYTES,
            });
        }
        let mut buf = [0u8; BYTES];
        buf[..b.len()].copy_from_slice(b);
        Ok(Self {
            buf,
            len: b.len() as u16,
        })
    }
}

impl<const BYTES: usize> TryFrom<&String> for InlineStr<BYTES> {
    type Error = InlineStrError;
    fn try_from(s: &String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl<const BYTES: usize> std::str::FromStr for InlineStr<BYTES> {
    type Err = InlineStrError;

    /// Identical to [`TryFrom<&str>`]. Present because axum's `Path<T>` extractor
    /// resolves a route segment through `FromStr`, which is what makes
    /// `/docs/{id}` work for a string-keyed model with nothing generated.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl<const BYTES: usize> std::ops::Deref for InlineStr<BYTES> {
    type Target = str;

    /// So the generated validation, filter and sort code keeps comparing against
    /// `&str` unchanged.
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<const BYTES: usize> AsRef<str> for InlineStr<BYTES> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// Comparison and hashing are defined on the TEXT, never on the buffer (#252
// res 8). The bytes past `len` are not part of the value: two values built from
// the same `&str` through different paths must be equal and must hash equal, and
// comparing `buf` wholesale would make that depend on what happened to be in the
// tail. This is a `HashMap` key type — `id_to_row`, `id_versions`, and #266's
// junction traversal indexes — so getting it wrong produces lookup misses rather
// than compile errors. The zeroed tail in the constructors is defence in depth,
// not the mechanism.
impl<const BYTES: usize> PartialEq for InlineStr<BYTES> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const BYTES: usize> Eq for InlineStr<BYTES> {}

impl<const BYTES: usize> std::hash::Hash for InlineStr<BYTES> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl<const BYTES: usize> Ord for InlineStr<BYTES> {
    /// Lexicographic by bytes, exactly as `str` orders — so an index range scan
    /// over a string key (#256) returns rows in the order the key text implies.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<const BYTES: usize> PartialOrd for InlineStr<BYTES> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// Cross-type comparison against the borrowed and owned string types, so a
// generated filter can compare a key column against a parsed query parameter
// without either side spelling a conversion.
impl<const BYTES: usize> PartialEq<str> for InlineStr<BYTES> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<const BYTES: usize> PartialEq<&str> for InlineStr<BYTES> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<const BYTES: usize> PartialEq<String> for InlineStr<BYTES> {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const BYTES: usize> PartialEq<InlineStr<BYTES>> for str {
    fn eq(&self, other: &InlineStr<BYTES>) -> bool {
        self == other.as_str()
    }
}

impl<const BYTES: usize> PartialEq<InlineStr<BYTES>> for String {
    fn eq(&self, other: &InlineStr<BYTES>) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const BYTES: usize> std::fmt::Display for InlineStr<BYTES> {
    /// The text. This is what the generated create handler's `id.to_string()`
    /// and the index key's `write!` produce, and what makes `/docs/{id}`
    /// resolvable.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const BYTES: usize> std::fmt::Debug for InlineStr<BYTES> {
    /// Renders as a quoted string, like `str` — a derived `Debug` would print
    /// `BYTES` integers into every log line and every panic message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<const BYTES: usize> Serialize for InlineStr<BYTES> {
    /// A JSON **string**, at every width.
    ///
    /// Do NOT replace this with `#[derive(Serialize)]`. serde's derive would
    /// serialize the `[u8; BYTES]` field, and its array impls stop at 32 — so a
    /// derive compiles for `InlineStr<26>` and fails to compile for
    /// `InlineStr<64>`. That is the #243 defect (`char(N)` above 32 could not be
    /// indexed) designed out at the type level rather than re-hit per width.
    fn serialize<S: serde::Serializer>(&self, ser: S) -> std::result::Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de, const BYTES: usize> Deserialize<'de> for InlineStr<BYTES> {
    /// From a JSON string, rejecting an over-capacity value rather than
    /// truncating it — see [`Serialize`] for why this is not a derive.
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> std::result::Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(de)?;
        Self::try_from(s.as_ref()).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod inline_str_internal_tests {
    use super::InlineStr;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    /// Res 8's tail clause, which cannot be written as an integration test: the
    /// public API has no way to produce a dirty tail (every constructor copies a
    /// whole `&str` into a zeroed buffer), by design. So the guard reaches
    /// through the private fields and builds one directly.
    ///
    /// Without this, `#[derive(PartialEq, Hash)]` would pass every other
    /// scenario — with a zeroed tail a buffer-wise comparison agrees with a
    /// text-wise one — and res 8 would be an unguarded claim.
    #[test]
    fn inline_str_tail_is_not_part_of_the_value() {
        let clean = InlineStr::<16> {
            buf: *b"ab\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            len: 2,
        };
        let dirty = InlineStr::<16> {
            buf: *b"abZZZZZZZZZZZZZZ",
            len: 2,
        };

        assert_eq!(clean.as_str(), "ab");
        assert_eq!(dirty.as_str(), "ab");
        assert_eq!(clean, dirty, "the tail must not participate in equality");
        assert_eq!(
            hash_of(&clean),
            hash_of(&dirty),
            "the tail must not participate in the hash"
        );
        assert_eq!(
            clean.cmp(&dirty),
            std::cmp::Ordering::Equal,
            "the tail must not participate in the ordering"
        );
    }

    /// The same clause for `Ord` across two different values, where a buffer-wise
    /// comparison would produce the *wrong* answer rather than merely a
    /// different-looking one: `"ab"` with a `Z` tail sorts above `"abc"` on bytes,
    /// but below it on text.
    #[test]
    fn inline_str_ordering_ignores_a_dirty_tail() {
        let ab_dirty = InlineStr::<8> {
            buf: *b"abZZZZZZ",
            len: 2,
        };
        let abc = InlineStr::<8> {
            buf: *b"abc\0\0\0\0\0",
            len: 3,
        };
        assert!(ab_dirty < abc);
    }
}
