Rewrites a model's column files to drop dead rows.

Holds the data directory and a [`CompactionConfig`]; `dead_space_threshold` is the only
config field this type reads, and only in [`Self::compact_needed`]. Construction does no
I/O. See [`crate`] for the on-disk layout and the ordering of a pass.
