use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

const US_PER_DAY: i64 = 86_400_000_000;
const US_PER_SEC: i64 = 1_000_000;

const RFC3339_MIN_US: i64 = -62_167_219_200_000_000;
const RFC3339_MAX_US: i64 = 253_402_300_799_999_999;

impl Timestamp {
    #[must_use]
    pub fn from_micros(micros: i64) -> Self {
        Timestamp(micros)
    }

    #[must_use]
    pub fn as_micros(&self) -> i64 {
        self.0
    }

    #[must_use]
    pub fn now() -> Self {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Timestamp(micros)
    }

    #[must_use]
    pub fn to_rfc3339(&self) -> String {
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

    pub fn from_rfc3339(s: &str) -> std::result::Result<Self, TimestampParseError> {
        parse_rfc3339(s).map(Timestamp).ok_or(TimestampParseError)
    }

    #[must_use]
    pub fn floor_to_micros(&self, quantum_us: i64) -> Self {
        if quantum_us <= 1 {
            return *self;
        }
        Timestamp(self.0.div_euclid(quantum_us) * quantum_us)
    }

    #[must_use]
    pub fn is_rfc3339_representable(&self) -> bool {
        (RFC3339_MIN_US..=RFC3339_MAX_US).contains(&self.0)
    }
}

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

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let mut i = 0usize;

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

    if i >= b.len() || !matches!(b[i], b'T' | b't' | b' ') {
        return None;
    }
    i += 1;

    let hour = take_digits(b, &mut i, 2)?;
    let minute = take_fixed(b, &mut i, b':', 2)?;
    let second = take_fixed(b, &mut i, b':', 2)?;

    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

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
    if i != b.len() {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let secs_of_day = hour * 3600 + minute * 60 + second - offset_seconds;
    days.checked_mul(US_PER_DAY)?
        .checked_add(secs_of_day.checked_mul(US_PER_SEC)?)?
        .checked_add(micros_of_second)
}

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

fn take_fixed(b: &[u8], i: &mut usize, sep: u8, n: usize) -> Option<i64> {
    if b.get(*i) != Some(&sep) {
        return None;
    }
    *i += 1;
    take_digits(b, i, n)
}

impl From<i64> for Timestamp {
    fn from(micros: i64) -> Self {
        Timestamp(micros)
    }
}

impl From<Timestamp> for i64 {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    U32(u32),
    U64(u64),
    I32(i32),
    I64(i64),
    F64(f64),
    Bool(bool),
    String(String),
    Uuid(Uuid),
    Timestamp(Timestamp),
}

impl Value {
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

    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Value::U32(_) | Value::U64(_) | Value::I32(_) | Value::I64(_) | Value::F64(_)
        )
    }

    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
}

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

#[derive(Clone, Copy)]
pub struct InlineStr<const BYTES: usize> {
    buf: [u8; BYTES],
    len: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineStrError {
    pub got_bytes: usize,
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
    pub const CAPACITY: usize = BYTES;

    const _LEN_FITS: () = assert!(
        BYTES <= u16::MAX as usize,
        "InlineStr records its length in a u16, so BYTES must be at most 65535",
    );

    #[must_use]
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.buf[..self.len as usize]) }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<const BYTES: usize> Default for InlineStr<BYTES> {
    fn default() -> Self {
        Self {
            buf: [0u8; BYTES],
            len: 0,
        }
    }
}

impl<const BYTES: usize> TryFrom<&str> for InlineStr<BYTES> {
    type Error = InlineStrError;

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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl<const BYTES: usize> std::ops::Deref for InlineStr<BYTES> {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<const BYTES: usize> AsRef<str> for InlineStr<BYTES> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

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
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<const BYTES: usize> PartialOrd for InlineStr<BYTES> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const BYTES: usize> std::fmt::Debug for InlineStr<BYTES> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<const BYTES: usize> Serialize for InlineStr<BYTES> {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> std::result::Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de, const BYTES: usize> Deserialize<'de> for InlineStr<BYTES> {
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
