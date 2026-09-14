A read snapshot: an immutable row-count watermark captured at a point in time.

This is the substrate primitive behind lock-free snapshot-isolated reads
(see the `mvcc-concurrency` proposal, Direction A). Because the storage
engine is **append-only** — rows are only ever appended, never moved or
mutated in place — a row's *position* is stable for its whole lifetime.
So a single integer, the row count at capture time, fully defines a
consistent view: exactly the rows whose index is below the watermark were
committed when the snapshot was taken, and every one of them is still at the
same index now.

A `Snapshot` carries **no** per-row version metadata and no reference to any
column data — it is a bare `usize`. Readers pass it to `*_at` accessors,
which clamp their scan to `visible(index)`. Rows appended after capture have
`index >= watermark` and are therefore invisible, so concurrent writers
cannot make a reader observe a torn or partial row.

It is schema-agnostic substrate: it knows nothing about any `.forge` model,
only about row positions. Generated code composes per-model watermarks into
its own snapshot struct.
