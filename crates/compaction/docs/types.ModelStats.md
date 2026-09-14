Row counts and byte usage of one model, as measured by
[`crate::StatsCollector::collect_model_stats`].

`total_disk_bytes`, `used_bytes` and `dead_bytes` are sums over `columns` plus
`index_sizes`; `index_sizes` is always empty, so in practice they are column sums.
Compaction measures `total_disk_bytes` before and after a pass to fill a
[`CompactionResult`].
