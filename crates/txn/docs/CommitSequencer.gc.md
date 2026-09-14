Prunes conflict entries that no live snapshot can still conflict with.

An entry recorded at `commit_lsn` is dropped once `commit_lsn` is below
[`Self::oldest_live_snapshot`]: no open snapshot predates that commit, so no future
[`Self::try_commit`] can carry a smaller `snapshot_lsn`. With no snapshot live at all the map
is cleared outright. Calling this periodically bounds the map to the keys modified since the
oldest live snapshot.
