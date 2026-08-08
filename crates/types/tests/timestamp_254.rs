//! #254 — `Timestamp` becomes microseconds, and its wire form becomes RFC 3339.
//!
//! Gate 3 scenarios 1–7. These guard the *substrate* half of the change: the
//! canonical unit, the rendering, the lenient parse, the floor, and the range
//! predicate that keeps `Display` total. Everything schema-facing (the declared
//! precision, the `us` identity floor, the migration) is guarded in the parser
//! and codegen crates — `Timestamp` deliberately knows nothing about a field's
//! declared precision (res 3).

use forgedb_types::Timestamp;

/// The canonical unit is microseconds, and `as_micros` is its only accessor.
/// `from_seconds`/`as_seconds` are REMOVED rather than deprecated (res 4): a
/// lossy `as_seconds` makes `from_seconds(as_seconds(t))` a silent precision
/// drop, which is the failure shape this whole issue exists to avoid.
#[test]
fn micros_is_the_canonical_unit() {
    let ts = Timestamp::from_micros(1_775_000_000_000_000);
    assert_eq!(ts.as_micros(), 1_775_000_000_000_000);
}

/// Scenario 1 — the rendering is RFC 3339 with exactly 6 fractional digits and a
/// trailing `Z`.
#[test]
fn renders_rfc3339_with_six_fractional_digits_and_z() {
    let ts = Timestamp::from_micros(1_775_000_000_123_456);
    assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.123456Z");
    // `Display` IS the rendering — one rendering everywhere (res 3), so the
    // create handler's `id.to_string()` and a REST path param agree with no
    // generated codec.
    assert_eq!(ts.to_string(), "2026-03-31T23:33:20.123456Z");
}

/// Scenario 2 — a second-aligned value (what a `timestamp(s)` field stores)
/// *still* renders 6 fractional digits. `Display` is on the type and cannot see
/// a field's declared precision; making the rendering schema-dependent would
/// either break the substrate's schema-agnosticism or force generated serde.
#[test]
fn a_second_aligned_value_still_renders_six_digits() {
    let ts = Timestamp::from_micros(1_775_000_000_000_000);
    assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.000000Z");
}

/// Scenario 3 — parsing is lenient: fewer (or more) fractional digits, a
/// lowercase `t`/`z`, and a non-`Z` offset all succeed. Output is fixed; input
/// is forgiving.
#[test]
fn parsing_is_lenient() {
    let cases: [(&str, i64); 6] = [
        ("2026-03-31T23:33:20Z", 1_775_000_000_000_000),
        ("2026-03-31T23:33:20.123Z", 1_775_000_000_123_000),
        ("2026-03-31T23:33:20.123456Z", 1_775_000_000_123_456),
        // Sub-microsecond digits are truncated, not rounded — the stored unit is
        // the finest thing the value can mean.
        ("2026-03-31T23:33:20.123456789Z", 1_775_000_000_123_456),
        // `+02:00` means the instant is two hours EARLIER in UTC.
        ("2026-04-01T01:33:20.123+02:00", 1_775_000_000_123_000),
        ("2026-03-31t23:33:20z", 1_775_000_000_000_000),
    ];
    for (src, expect) in cases {
        let ts: Timestamp = src.parse().unwrap_or_else(|e| panic!("{src}: {e:?}"));
        assert_eq!(ts.as_micros(), expect, "{src}");
        assert_eq!(
            Timestamp::from_rfc3339(src).unwrap().as_micros(),
            expect,
            "{src} — `from_rfc3339` and `FromStr` are one parse"
        );
    }
}

/// Scenario 4 — a pre-epoch (negative) value survives `Display` → `FromStr`
/// unchanged. Negative micros are where a naive `/`-based civil-date conversion
/// goes wrong, so this is not a formality.
#[test]
fn a_pre_epoch_value_round_trips() {
    for us in [-1_i64, -1_000_000, -123_456, -86_400_000_000, -2_208_988_800_000_000] {
        let ts = Timestamp::from_micros(us);
        let back: Timestamp = ts.to_string().parse().unwrap();
        assert_eq!(back.as_micros(), us, "{us} round-trips through {}", ts);
    }
    // The value one microsecond before the epoch is 1969-12-31T23:59:59.999999Z,
    // NOT 1970-01-01T00:00:00.-000001Z.
    assert_eq!(
        Timestamp::from_micros(-1).to_rfc3339(),
        "1969-12-31T23:59:59.999999Z"
    );
}

/// Scenario 5 — the type's *representable* range is RFC 3339's year range
/// (0000–9999), which is narrower than `i64` micros (±292,277 years). The
/// predicate is what the generated write path checks before storing; `Display`
/// itself cannot fail, so the rejection has to live at the write boundary.
#[test]
fn the_rfc3339_range_is_the_types_range() {
    // 9999-12-31T23:59:59Z, the last representable second.
    assert!(Timestamp::from_micros(253_402_300_799_999_999).is_rfc3339_representable());
    assert!(Timestamp::from_micros(253_402_300_800_000_000).is_rfc3339_representable() == false);
    // 0000-01-01T00:00:00Z.
    assert!(Timestamp::from_micros(-62_167_219_200_000_000).is_rfc3339_representable());
    assert!(Timestamp::from_micros(-62_167_219_200_000_001).is_rfc3339_representable() == false);
    assert!(Timestamp::from_micros(0).is_rfc3339_representable());
    assert!(Timestamp::now().is_rfc3339_representable());
}

/// Scenario 6 — quantizing FLOORS, it never truncates toward zero. For a
/// pre-epoch value truncation rounds *forward* in time, which breaks
/// monotonicity across 1970 — the one case a `/` would get wrong.
#[test]
fn quantizing_floors_it_never_truncates_toward_zero() {
    const SEC: i64 = 1_000_000;
    // Positive: 12:00:00.750000 floors to 12:00:00.
    assert_eq!(
        Timestamp::from_micros(1_775_000_000_750_000)
            .floor_to_micros(SEC)
            .as_micros(),
        1_775_000_000_000_000
    );
    // Negative: -0.5s floors to -1s. Truncation toward zero would give 0, i.e.
    // a LATER instant than the input.
    assert_eq!(
        Timestamp::from_micros(-500_000).floor_to_micros(SEC).as_micros(),
        -1_000_000
    );
    assert_eq!(
        Timestamp::from_micros(-1).floor_to_micros(SEC).as_micros(),
        -1_000_000
    );
    // Monotonicity across the epoch boundary: flooring never reorders two values.
    let mut last = i64::MIN;
    for us in [-2_500_000_i64, -1_500_000, -500_000, 0, 500_000, 1_500_000] {
        let f = Timestamp::from_micros(us).floor_to_micros(SEC).as_micros();
        assert!(f >= last, "flooring reordered {us}");
        assert!(f <= us, "flooring moved {us} forward in time");
        last = f;
    }
    // A quantum of 1 (the storage unit) is the identity — which is why a
    // `+timestamp(us)` identity has nothing to quantize.
    assert_eq!(
        Timestamp::from_micros(-7).floor_to_micros(1).as_micros(),
        -7
    );
}

/// Scenario 7 — a malformed string is an `Err`, never a panic. Every one of
/// these reaches the parser from a REST path segment or a query param, so a
/// panic here is a remote crash.
#[test]
fn a_malformed_string_is_an_error_never_a_panic() {
    let bad = [
        "",
        "nonsense",
        "2026-03-31",
        "2026-03-31T23:33",
        "2026-13-01T00:00:00Z",
        "2026-00-01T00:00:00Z",
        "2026-02-30T00:00:00Z",
        "2026-03-31T24:00:00Z",
        "2026-03-31T23:60:00Z",
        "2026-03-31T23:33:60Z",
        "2026-03-31T23:33:20",
        "2026-03-31T23:33:20+2:00",
        "2026-03-31T23:33:20.Z",
        "2026-03-31X23:33:20Z",
        "202-03-31T23:33:20Z",
        "9223372036854775807",
        "2026-03-31T23:33:20Z ",
    ];
    for src in bad {
        assert!(
            src.parse::<Timestamp>().is_err(),
            "{src:?} must not parse to a Timestamp"
        );
        assert!(Timestamp::from_rfc3339(src).is_err(), "{src:?}");
    }
}

/// `now()` is in the canonical unit. A seconds-valued `now()` would put every
/// freshly stamped row in 1970 — the exact silent-wrong-date failure.
#[test]
fn now_is_micros() {
    let now = Timestamp::now().as_micros();
    // 2020-01-01T00:00:00Z in micros. A seconds-valued clock reads ~1.7e9 here.
    assert!(now > 1_577_836_800_000_000, "now() must be micros, got {now}");
}

/// The serialized form is the RFC 3339 string, not a bare integer (res 3). A
/// client that expected a number now fails immediately and visibly instead of
/// silently displaying 1970 forever.
#[test]
fn serde_is_the_rfc3339_string() {
    let ts = Timestamp::from_micros(1_775_000_000_123_456);
    let json = serde_json::to_string(&ts).unwrap();
    assert_eq!(json, "\"2026-03-31T23:33:20.123456Z\"");
    let back: Timestamp = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ts);
    // A bare number is no longer a Timestamp — the break is loud by design.
    assert!(serde_json::from_str::<Timestamp>("1775000000").is_err());
}

/// Ordering is still the numeric ordering of the underlying micros, unaffected
/// by the wire form — the body's "storage and index keys stay `i64` micros".
#[test]
fn ordering_is_numeric_not_lexicographic() {
    let a = Timestamp::from_micros(-1);
    let b = Timestamp::from_micros(0);
    let c = Timestamp::from_micros(1_775_000_000_000_000);
    assert!(a < b && b < c);
}
