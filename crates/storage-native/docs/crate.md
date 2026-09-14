Native columnar storage backend for ForgeDB: positional file I/O over per-column files, a physical-layout manifest, a byte-per-row tombstone file, and an advisory single-writer directory lock.

`forgedb-storage-native` is the engine the `forgedb-storage` facade re-exports on non-`wasm32` targets. It is schema-agnostic substrate: it never reads a `.forge` schema and knows nothing about models or fields. Generated code decides the directory layout, opens each column by path, and drives the typed `append_*` / `read_*` calls directly.

# What the crate provides

- [`FixedColumn`]: one file of fixed-width values, `value_size` bytes per row, addressed as `offset = index * value_size`.
- [`VariableColumn`]: a data file of concatenated value bytes plus an offsets file holding one little-endian `(offset: u64, length: u64)` pair per row.
- [`Tombstones`]: one byte per row, `0` live and `1` deleted, so a delete moves no data and row indices stay stable.
- [`FixedColumnReader`], [`VariableColumnReader`], [`TombstonesReader`]: read-only views over the same files through independent descriptors, for concurrent readers beside a single writer.
- [`BufferedFixedColumn`], [`BufferedVariableColumn`], [`ColumnExport`]: a row selection loaded into memory (a copy or an `mmap` alias) for column scans and columnar export.
- [`Manifest`], [`ColumnMetadata`], [`RowAnchor`]: the physical-layout metadata persisted as `manifest.json` beside the column files, so schema-blind tooling (backup, the inspector) can bound every file without the schema.
- [`Snapshot`]: a row-count watermark that defines a consistent read view over append-only columns.
- [`DirLock`]: an exclusive advisory lock on a data directory.

# I/O model

Reads are positional (`pread`-style `read_exact_at`) and take `&self`; they never touch the file cursor, so any number of readers can run concurrently on one handle or across cloned handles. Appends take `&mut self`, seek to the end of the file and write, so a column has one writer at a time. Each writer handle tracks its row count in memory; `sync_from_disk` re-derives it from the file when another process may have appended.

All multi-byte values are stored little-endian.

# Layout on disk

The crate takes paths; it does not name files. The layout generated code writes for one model looks like this:

```text
<model>/
├── manifest.json               physical layout (Manifest)
├── tombstones.bin              one byte per row, appended last per insert
├── fixed/
│   └── uuid_0.bin              fixed-width column, value_size bytes per row
└── variable/
    ├── string_data_1.bin       concatenated value bytes
    └── string_offsets_1.bin    one (offset, length) pair per row, 16 bytes
```

# Durability

Column files do **not** fsync on every append. Appended bytes are visible to subsequent reads through the page cache, but a crash before an explicit [`FixedColumn::flush`] / [`VariableColumn::flush`] / [`Tombstones::flush`] can lose the most recent appends. The write-ahead log is the crash-durability boundary: record a mutation with [`WalManager`] before applying it to the column files, replay the log on recovery, and call `truncate_to_rows` on every column to realign them to the last consistent row count.

For a checkpoint spanning many files, `sync_to_drive` on each file followed by one `barrier` on any file of the same device makes the whole set durable with a single device-cache flush instead of one per file.

# Locking

[`DirLock::acquire`] takes an exclusive advisory lock on `<root>/.forgedb.lock`; a second acquire, from this or another process, fails with `WouldBlock`. It prevents two writers from opening one directory by accident; it is not a lease, a registry or a coordinator, and it does not serialize concurrent writers. The `forgedb-coordinator` process locks the same filename, so a coordinator and a standalone writer are mutually exclusive on one directory.

# WAL re-exports

[`FsyncPolicy`], [`WalEntry`], [`WalManager`] and [`WalOperation`] are re-exported from [`forgedb_wal`] so generated code needs a single storage import.
