Parses an `order` value: `asc` or `ascending` give [`Self::Asc`], `desc` or `descending` give [`Self::Desc`], compared case-insensitively; anything else, including surrounding whitespace, is `None`.

This is an inherent method, not an implementation of the `FromStr` trait.
