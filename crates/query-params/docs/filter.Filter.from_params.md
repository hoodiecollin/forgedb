Turns every non-reserved entry of a decoded parameter map into a filter.

The keys `sort`, `order`, `limit` and `offset` are dropped; each other entry becomes one filter whose value is classified from the text, in this order:

- [`FilterValue::Number`] when the text parses as a finite `f64` **and** that number's `Display` form is the original text, so `30`, `-2` and `3.5` are numbers while `1.0`, `+5`, `007`, `1e3`, `.5`, `inf` and `NaN` are not.
- [`FilterValue::Bool`] when the text is exactly `true` or `false` (`True` is a string).
- [`FilterValue::String`] otherwise, including the empty string.

The map's keys are unique, so there is at most one filter per field; the output order follows the map's iteration order and is unspecified.
