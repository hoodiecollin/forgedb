Flush pending writes to disk with `File::sync_all`.

After it returns `Ok(())`, every previous append survives a crash. Call it at commit boundaries or before advancing the WAL checkpoint.
