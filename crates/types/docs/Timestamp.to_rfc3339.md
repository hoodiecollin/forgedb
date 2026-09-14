Renders the instant as RFC 3339 with exactly six fractional digits and a `Z` suffix.

Total by construction: this is what [`Display`](std::fmt::Display) calls. A year outside
0000–9999 renders with a sign and no width limit (`-0001-…`, `+10000-…`) so the round trip
through [`Self::from_rfc3339`] stays lossless; refuse such values with
[`Self::is_rfc3339_representable`] where a strict RFC 3339 string is required.
