Read-only, positional view over a [`VariableColumn`]'s data and offsets files, created by [`VariableColumn::reader`].

Same concurrency model as [`FixedColumnReader`]: independent descriptors, positional reads, length derived from the offsets file on every call, no cached bound.
