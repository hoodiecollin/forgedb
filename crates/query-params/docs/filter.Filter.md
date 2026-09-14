One `field=value` query parameter: an opaque field name paired with a typed [`FilterValue`].

The crate does not know whether the field exists or what type it has; the `matches_*` methods compare the parsed value against a candidate of one Rust type, and the caller picks the method that matches the field's actual type. Equality is the only comparison, and it never crosses variants: a filter parsed as a number does not match a string, even one with the same digits.
