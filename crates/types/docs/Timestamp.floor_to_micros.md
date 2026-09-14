Floors the instant to a multiple of `quantum_us` microseconds.

This is how a value finer than a field's declared precision is recorded coarsely rather than
refused. It floors toward negative infinity, never toward zero: truncating toward zero would
move a pre-epoch value forward in time and could reorder two instants on opposite sides of
1970. A `quantum_us` of `1` or less returns the value unchanged.
