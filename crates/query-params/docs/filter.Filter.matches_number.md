`true` when the value is a [`FilterValue::Number`] equal to `value` under exact `f64` equality; `false` for any other variant.

There is no tolerance: a REST filter is a discrete user-supplied value such as `age=30`, and only the same number matches it. `NaN` never matches.
