Attempts to commit a write-set under first-committer-wins.

Every key in `ws.keys` is checked against the LSN it was last committed at. If any key's last
commit is strictly greater than `ws.snapshot_lsn`, the result is [`CommitOutcome::Conflict`]
naming the first such key, and nothing is recorded: the caller discards its staged writes and
retries from a fresh snapshot. Otherwise the sequencer assigns the next LSN, records it for
every key in the write-set, and returns [`CommitOutcome::Committed`].

The check is opaque-key equality plus an integer compare. Keys are never interpreted.
