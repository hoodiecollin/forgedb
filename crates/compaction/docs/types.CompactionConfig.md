Tunables for compaction.

`Default` gives `dead_space_threshold: 0.3`, `auto_compact: true`, `check_interval_secs:
300`, `max_compaction_time_secs: 600`. Only two fields are read: `dead_space_threshold` by
[`ModelStats::needs_compaction`] and `check_interval_secs` by [`crate::BackgroundCompactor`].
Generated code always uses the default.
