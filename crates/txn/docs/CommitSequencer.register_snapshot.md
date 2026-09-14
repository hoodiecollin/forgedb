Register a read snapshot.

Returns the LSN of the last committed transaction (`next_lsn - 1`), which
is `Lsn(start_lsn)` if nothing has committed yet.  Increments the refcount
for that LSN so [`gc`] knows it is still needed.

The caller must pair every `register_snapshot` with exactly one
[`release_snapshot`] on the same LSN to keep refcounts correct.
