Renders the instant as RFC 3339: exactly 6 fractional digits, always `Z`.

Total by construction — this is what [`Display`] calls, and `Display`
cannot fail. A value outside RFC 3339's year range renders with an
extended year (`+10000-…` / `-0001-…`) so the round-trip stays lossless;
such a value is refused at the write boundary instead, via
[`is_rfc3339_representable`](Self::is_rfc3339_representable).
