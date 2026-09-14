The write-set a transaction hands to [`CommitSequencer::try_commit`].

`keys` is the complete set of opaque row and unique-key handles the
transaction touched.  `snapshot_lsn` is the LSN at which the transaction
took its read snapshot (i.e., `CommitSequencer::register_snapshot`'s return
value); any key last committed at a strictly-higher LSN is a conflict.
