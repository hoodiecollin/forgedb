Attempt to commit a transaction's write-set (first-committer-wins, #83).

For each key in `ws.keys`, checks whether it was last committed at a
LSN strictly greater than `ws.snapshot_lsn`.  If ANY key conflicts,
returns [`CommitOutcome::Conflict`] immediately (the caller must discard
staged writes and retry).  On success, assigns the next monotonic LSN,
records it for every key in the write-set, and returns
[`CommitOutcome::Committed`].

Pure opaque-key equality + integer compare — no schema, no model name,
no field awareness.
