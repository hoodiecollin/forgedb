use forgedb_types::Timestamp;

#[test]
fn micros_is_the_canonical_unit() {
    let ts = Timestamp::from_micros(1_775_000_000_000_000);
    assert_eq!(ts.as_micros(), 1_775_000_000_000_000);
}

#[test]
fn renders_rfc3339_with_six_fractional_digits_and_z() {
    let ts = Timestamp::from_micros(1_775_000_000_123_456);
    assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.123456Z");
    assert_eq!(ts.to_string(), "2026-03-31T23:33:20.123456Z");
}

#[test]
fn a_second_aligned_value_still_renders_six_digits() {
    let ts = Timestamp::from_micros(1_775_000_000_000_000);
    assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.000000Z");
}

#[test]
fn parsing_is_lenient() {
    let cases: [(&str, i64); 6] = [
        ("2026-03-31T23:33:20Z", 1_775_000_000_000_000),
        ("2026-03-31T23:33:20.123Z", 1_775_000_000_123_000),
        ("2026-03-31T23:33:20.123456Z", 1_775_000_000_123_456),
        ("2026-03-31T23:33:20.123456789Z", 1_775_000_000_123_456),
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

#[test]
fn a_pre_epoch_value_round_trips() {
    for us in [-1_i64, -1_000_000, -123_456, -86_400_000_000, -2_208_988_800_000_000] {
        let ts = Timestamp::from_micros(us);
        let back: Timestamp = ts.to_string().parse().unwrap();
        assert_eq!(back.as_micros(), us, "{us} round-trips through {}", ts);
    }
    assert_eq!(
        Timestamp::from_micros(-1).to_rfc3339(),
        "1969-12-31T23:59:59.999999Z"
    );
}

#[test]
fn the_rfc3339_range_is_the_types_range() {
    assert!(Timestamp::from_micros(253_402_300_799_999_999).is_rfc3339_representable());
    assert!(Timestamp::from_micros(253_402_300_800_000_000).is_rfc3339_representable() == false);
    assert!(Timestamp::from_micros(-62_167_219_200_000_000).is_rfc3339_representable());
    assert!(Timestamp::from_micros(-62_167_219_200_000_001).is_rfc3339_representable() == false);
    assert!(Timestamp::from_micros(0).is_rfc3339_representable());
    assert!(Timestamp::now().is_rfc3339_representable());
}

#[test]
fn quantizing_floors_it_never_truncates_toward_zero() {
    const SEC: i64 = 1_000_000;
    assert_eq!(
        Timestamp::from_micros(1_775_000_000_750_000)
            .floor_to_micros(SEC)
            .as_micros(),
        1_775_000_000_000_000
    );
    assert_eq!(
        Timestamp::from_micros(-500_000).floor_to_micros(SEC).as_micros(),
        -1_000_000
    );
    assert_eq!(
        Timestamp::from_micros(-1).floor_to_micros(SEC).as_micros(),
        -1_000_000
    );
    let mut last = i64::MIN;
    for us in [-2_500_000_i64, -1_500_000, -500_000, 0, 500_000, 1_500_000] {
        let f = Timestamp::from_micros(us).floor_to_micros(SEC).as_micros();
        assert!(f >= last, "flooring reordered {us}");
        assert!(f <= us, "flooring moved {us} forward in time");
        last = f;
    }
    assert_eq!(
        Timestamp::from_micros(-7).floor_to_micros(1).as_micros(),
        -7
    );
}

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

#[test]
fn now_is_micros() {
    let now = Timestamp::now().as_micros();
    assert!(now > 1_577_836_800_000_000, "now() must be micros, got {now}");
}

#[test]
fn serde_is_the_rfc3339_string() {
    let ts = Timestamp::from_micros(1_775_000_000_123_456);
    let json = serde_json::to_string(&ts).unwrap();
    assert_eq!(json, "\"2026-03-31T23:33:20.123456Z\"");
    let back: Timestamp = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ts);
    assert!(serde_json::from_str::<Timestamp>("1775000000").is_err());
}

#[test]
fn ordering_is_numeric_not_lexicographic() {
    let a = Timestamp::from_micros(-1);
    let b = Timestamp::from_micros(0);
    let c = Timestamp::from_micros(1_775_000_000_000_000);
    assert!(a < b && b < c);
}
