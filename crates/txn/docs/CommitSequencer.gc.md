Prune conflict-map entries that no live snapshot can still conflict against.

An entry `(key, commit_lsn)` is safe to drop when `commit_lsn <
oldest_live_snapshot()`: no open snapshot predates `oldest`, so no future
challenger will have a `snapshot_lsn < commit_lsn` for that entry.  Calling
this periodically bounds the conflict map to O(keys modified since the oldest
live snapshot), which is typically small under the single-writer Tier-2 model.
