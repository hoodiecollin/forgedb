Check if this filter matches a number value.

Uses exact equality. REST filter params represent discrete user-supplied values
(e.g. `age=30`), where exact bit-level equality is the correct semantic.
The previous relative-epsilon comparison was wrong for large magnitudes and
added false positives for nearby-but-distinct values.
