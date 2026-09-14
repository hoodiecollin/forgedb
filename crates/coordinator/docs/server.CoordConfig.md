Tunables bound once when a coordinator opens; none can change while it runs.

All three are schema-blind. `Default` is [`CoordFsync::Always`], [`TURN_TIMEOUT`] and [`crate::DEFAULT_MAX_FRAME`]. The `forgedb coordinate` command sets them from `--fsync`/`--fsync-interval`, `--turn-timeout` and `--max-frame-mib`, or from the `FORGEDB_COORDINATOR_FSYNC`, `FORGEDB_COORDINATOR_FSYNC_INTERVAL`, `FORGEDB_COORDINATOR_TURN_TIMEOUT` and `FORGEDB_COORDINATOR_MAX_FRAME_MIB` environment variables.
