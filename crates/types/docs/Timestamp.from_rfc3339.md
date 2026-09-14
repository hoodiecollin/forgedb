Parses an RFC 3339 instant, accepting more than [`Self::to_rfc3339`] emits.

Accepted: a four-digit year, or a signed year of four or more digits; `T`, `t` or a space
between date and time; an optional fraction of any length, of which the first six digits are
kept and the rest truncated; and `Z`, `z`, or a numeric offset written `±HH:MM`, `±HHMM` or
`±HH`. The offset is applied, not recorded: the result is an instant. Month, day, hour, minute
and second are range-checked (seconds up to 59, so a leap second is refused). Anything else
is a [`TimestampParseError`].
