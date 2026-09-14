Registers a read snapshot and returns the LSN of the last commit.

The returned value is `next_lsn - 1`, so it is `Lsn(start_lsn)` when nothing has committed
yet. The sequencer increments a reference count for that LSN so [`Self::gc`] keeps every
conflict entry the snapshot could still collide with.

Pair every call with exactly one [`Self::release_snapshot`] on the same LSN.
