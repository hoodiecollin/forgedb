`true` when `dead_space_ratio` is at or above `config.dead_space_threshold`.

The selection rule behind [`DatabaseStats::models_needing_compaction`] and
[`crate::Compactor::compact_needed`].
