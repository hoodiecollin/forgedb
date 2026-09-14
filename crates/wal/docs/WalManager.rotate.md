Archive the current file and start an empty one at the same path.

The archive is the WAL path with its extension replaced by
`log.<seconds since the Unix epoch>` (so `wal.log` becomes
`wal.log.1700000000`), produced by fsyncing and then renaming the current
file. A fresh writer with the same fsync policy and a fresh reader are then
opened at the original path. Returns the archive path. Two rotations within
the same second produce the same archive name, and the second rename replaces
the first archive.
