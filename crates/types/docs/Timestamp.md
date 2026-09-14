Microseconds elapsed since the Unix epoch (January 1, 1970 00:00:00 UTC).

# The unit is microseconds (#254)

Before v0.4.0 this held whole **seconds**. It now holds **microseconds**, and
microseconds are the one canonical on-disk unit regardless of a field's
declared `timestamp(s|ms|us)` precision — a `timestamp(ms)` field simply
stores millisecond-aligned microsecond values. Storing each field in its own
unit would make a `Timestamp` value unit-ambiguous at runtime, which is only
resolvable with a fatter value (a layout change) or a `Timestamp<const U>`
(generated signature churn everywhere).

`i64` microseconds spans ±292,277 years, which is why microseconds rather
than nanoseconds are the floor: nanoseconds would cap the type at 1678–2262,
putting a birth date or a long-dated bond maturity out of range.

# The wire form is RFC 3339, not a number

[`Display`], [`FromStr`](std::str::FromStr) and serde all speak RFC 3339 —
one rendering everywhere, so the create handler's `id.to_string()` and a REST
path parameter's `parse()` agree with no generated codec. Output is fixed
(**exactly 6 fractional digits, always `Z`**, including for a `timestamp(s)`
field: `Display` is on the type and cannot see a field's precision); input is
lenient (fewer or more fractional digits, lowercase `t`/`z`, any numeric
offset).

A number would have been a *silent* break — a client reading seconds would
start reading microseconds and display 1970 forever. A client expecting a
number and receiving a string fails immediately and visibly.

# Range

The rendering has no failure mode, so the **type's** range is RFC 3339's year
range 0000–9999 ([`is_rfc3339_representable`](Timestamp::is_rfc3339_representable)),
enforced by the generated write path rather than here. Values outside it
still render and re-parse losslessly (with an extended year), so no data is
trapped; they are simply refused at the boundary.

# Examples

```rust
use forgedb_types::Timestamp;

let ts = Timestamp::from_micros(1_775_000_000_123_456);
assert_eq!(ts.to_rfc3339(), "2026-03-31T23:33:20.123456Z");
assert_eq!(ts.as_micros(), 1_775_000_000_123_456);
assert_eq!("2026-03-31T23:33:20.123456Z".parse::<Timestamp>().unwrap(), ts);
```
