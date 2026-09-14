Builds a window from optional `limit` and `offset` parameters.

A missing `limit` is [`DEFAULT_LIMIT`] and the result is clamped to `1..=`[`MAX_LIMIT`]; a missing `offset` is `0`. [`crate::QueryParams::from_map`] passes `None` for a parameter that is absent or does not parse as a `usize`.
