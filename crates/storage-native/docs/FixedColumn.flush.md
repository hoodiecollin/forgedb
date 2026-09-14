Flush all pending writes to disk (fsync).

This is the explicit durability checkpoint: after `flush()` returns `Ok(())`,
all previous appends are guaranteed to survive a crash. Call this at transaction
commit boundaries or before advancing the WAL checkpoint.
