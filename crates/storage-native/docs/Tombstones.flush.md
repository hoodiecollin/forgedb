Flush all pending writes to disk (fsync).

After `flush()` returns `Ok(())`, all previous appends are guaranteed to
survive a crash. Call at commit boundaries or before advancing the WAL checkpoint.
