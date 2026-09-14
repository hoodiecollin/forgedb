A read snapshot: an immutable row-count watermark captured at a point in time.

Because the column files are append-only (rows are appended, never moved or rewritten in place), a row's index is stable for its lifetime, so a single integer defines a consistent view: exactly the rows with index below the watermark were committed at capture, and each is still at the same index now. Rows appended later have `index >= watermark` and are invisible, so a concurrent writer cannot make a reader observe a partial row.

It carries no per-row version metadata and no reference to column data; it is a bare `usize` that knows nothing about any model. Generated code composes per-model watermarks into its own snapshot type and clamps its reads with [`Snapshot::visible`].
