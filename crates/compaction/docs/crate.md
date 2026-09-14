Dead-space reclaim and storage statistics for a ForgeDB data directory.

`forgedb-compaction` is schema-agnostic substrate: it rewrites the column files under a
model directory as opaque bytes and reads no `.forge` schema. Which rows are live is the
caller's decision.

# What generated code links

A generated database's `Database::compact()` builds a [`Compactor`] over its data directory
with a default [`CompactionConfig`] and calls [`Compactor::compact_model_keeping`] with the
physical row indices it wants to keep. That is the only entry point generated code uses.
[`BackgroundCompactor`], [`MaintenanceApi`] and the tombstone-based
[`Compactor::compact_model`] are not linked by generated code.

# Two reclaim primitives

- [`Compactor::compact_model_keeping`] keeps exactly the rows the caller names and drops
  everything else. This is the supported path.
- [`Compactor::compact_model`] drops the rows whose tombstone byte is set and keeps the
  rest. It is deprecated: a generated database records a delete as a tombstoned marker row
  and an update as a newly appended version, so tombstone-driven compaction reclaims nothing
  from updates and brings deleted rows back. [`Compactor::compact_all`],
  [`Compactor::compact_needed`], [`BackgroundCompactor`] and the compaction methods of
  [`MaintenanceApi`] all route through it. The `forgedb compact` and `forgedb vacuum` CLI
  commands refuse with an error that points at the in-process `Database::compact()`.

# On-disk layout

Every method takes a data directory and addresses a model by the name of its subdirectory:

```text
<data_dir>/<model>/
  tombstones.bin                  one byte per row; nonzero = deleted; its length is the row count
  manifest.json                   optional; its row_count is rewritten after a pass
  fixed/<column>.bin              fixed-width column; row width = file size / row count
  variable/<column>_data.bin      variable-length column payload
  variable/<column>_offsets.bin   one little-endian (offset: u64, length: u64) pair per row
  .last_compaction                unix seconds of the last compaction pass
```

A model directory without `tombstones.bin` can be neither compacted nor measured: the row
count is that file's size.

# A compaction pass

A pass reads the model's statistics, rewrites every column file into a `.bin.tmp` sibling
holding only the kept rows, renames each temp file over its original, rewrites
`tombstones.bin` as one zero byte per surviving row, updates `row_count` in
`manifest.json` when that file exists, writes `.last_compaction`, and re-reads the
statistics to fill a [`CompactionResult`]. Each rename is atomic but the renames are
sequential: a crash between two of them leaves those columns at different versions.

# Locking

The compactor takes no lock. The caller must ensure nothing else reads or writes the model
directory during a pass; a generated database runs it under its single-writer lock and
rebuilds its in-memory row mapping afterwards.

# Statistics

[`StatsCollector`] produces [`DatabaseStats`], [`ModelStats`] and [`ColumnStats`] from the
same layout without modifying it. [`ModelStats::needs_compaction`] compares a model's
dead-space ratio against [`CompactionConfig::dead_space_threshold`].
