The write-set a transaction hands to [`CommitSequencer::try_commit`].

`keys` is the complete set of opaque row and unique-key handles the transaction wrote.
`snapshot_lsn` is the value [`CommitSequencer::register_snapshot`] returned when the
transaction began; any key last committed at a strictly higher LSN is a conflict.
