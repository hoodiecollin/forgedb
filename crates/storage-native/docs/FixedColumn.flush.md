Flush pending writes to disk with `File::sync_all`.

After it returns `Ok(())`, every previous append survives a crash. Call it at commit boundaries or before advancing the WAL checkpoint. For a checkpoint over many files, [`FixedColumn::sync_to_drive`] on each followed by one [`FixedColumn::barrier`] is the cheaper form.
