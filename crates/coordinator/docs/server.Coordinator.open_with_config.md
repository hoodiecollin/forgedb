Like [`open`](Self::open) but with an explicit [`CoordConfig`] (#144/#145,
epic #126): the replication-log fsync policy, the turn-reclaim timeout, and
the max protocol frame. `CoordConfig::default()` reproduces the prior
behavior (Always fsync, 30s turn timeout, 16 MiB frame cap).
