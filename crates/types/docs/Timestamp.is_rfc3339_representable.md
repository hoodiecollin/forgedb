Whether the value renders inside RFC 3339's year range (0000–9999).

`i64` microseconds spans ±292,277 years, so the wire form is narrower than
the storage type. Since [`Display`] cannot fail, the narrowing is enforced
as a validity constraint at the write boundary (a 422) rather than as a
rendering error — and this predicate is what the generated write path
checks.
