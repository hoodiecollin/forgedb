Creates a stopped compactor over `data_dir`.

Builds a [`Compactor`] with `config`; only `check_interval_secs` and
`dead_space_threshold` are read. No thread is spawned until [`Self::start`], and the
status starts as [`CompactionStatus::Idle`].
