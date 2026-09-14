Whether the instant falls inside RFC 3339's year range, 0000 through 9999.

`i64` microseconds reach far beyond that range and [`Self::to_rfc3339`] cannot fail, so the
narrowing is a predicate for the caller to check at a validation boundary rather than a
rendering error.
