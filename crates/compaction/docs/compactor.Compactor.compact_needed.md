Compacts only the models whose dead-space ratio is at or above
[`CompactionConfig::dead_space_threshold`], through the deprecated [`Self::compact_model`].

Collects [`DatabaseStats`] first (a model whose statistics cannot be read is logged and
skipped), then compacts each model that [`ModelStats::needs_compaction`] selects. A failed
compaction contributes a result with `success == false` rather than aborting. Returns `Err`
only when the data directory cannot be listed. This is the pass
[`crate::BackgroundCompactor`] runs on its interval.
