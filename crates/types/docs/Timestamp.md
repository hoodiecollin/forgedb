An instant, stored as microseconds since the Unix epoch (1970-01-01T00:00:00Z).

## The unit is microseconds

Microseconds are the one on-disk unit whatever precision a field declares: a `timestamp(ms)`
field stores millisecond-aligned microsecond values (see [`Self::floor_to_micros`]). An `i64`
of microseconds spans about ±292,000 years, which is why microseconds rather than nanoseconds
are the floor: nanoseconds would confine the type to 1678–2262.

## The wire form is RFC 3339, not a number

[`Display`](std::fmt::Display), [`FromStr`](std::str::FromStr) and serde all speak RFC 3339, so
`to_string()` and `parse()` agree everywhere without a generated codec. Output is fixed: exactly
six fractional digits and a trailing `Z`, whatever the field's declared precision. Input is
lenient: any number of fractional digits (extra ones are truncated), a lowercase `t`, `z` or a
space separator, and any numeric offset.

## Range

Rendering cannot fail, so the type's range is RFC 3339's year range 0000–9999
([`Self::is_rfc3339_representable`]). A value outside it still renders and re-parses
losslessly, with a signed extended year; refusing such values is the write boundary's job,
not this type's.

Ordering, equality and hashing are on the inner microsecond count.
