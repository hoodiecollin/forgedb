Fixed-size column storage backed by a seek-based file.

# Durability

Append methods write to the OS page cache but do **not** call `fsync` per append.
Data written by `append_*` is immediately readable (via `read_*`) on the same or
any other file descriptor to the same file, but a crash before [`flush`](FixedColumn::flush)
may lose the most recent appends. Call `flush()` at commit boundaries or before
advancing the WAL checkpoint.
The backing of one columnar export batch — the substrate half of the
language bindings' Arrow / columnar-export path.

A columnar export hands the caller (generated FFI glue) a single contiguous,
stable-pointer byte buffer for one column at one live-row selection. This
type owns that buffer through *whichever* of the two production paths
produced it, behind an identical `as_ptr()` / `len()` surface so the consumer
(e.g. an Arrow `ArrowArray`) is **alias-or-gather transparent**:

- [`ColumnExport::Mapped`] — a **zero-copy `mmap` alias** of the column
  file's dense prefix, taken when the live selection is exactly the
  contiguous prefix `[0, n)` (no updates, no tombstones). No bytes are
  copied; the pointer aliases the page cache directly. Because numeric
  columns are stored little-endian (`to_le_bytes`), the mapped prefix *is* a
  valid Arrow primitive buffer as-is.
- [`ColumnExport::Owned`] — a gathered contiguous copy (`FixedColumn::gather`),
  the fallback whenever the selection is not an aliasable dense prefix
  (update-heavy or tombstoned tables), or is empty.

# Safety / aliasing invariant (`Mapped`)

The mapped prefix aliases live file pages, so it relies on the same
single-writer, append-only discipline the reader handles already assume:
prefix rows `[0, n)` are committed and immutable (appends only ever extend
past `n`, never rewrite mapped bytes), and a compaction/rollback that would
renumber or truncate below `n` does not run while an export is outstanding
(exports are synchronous reads taken on the single writer). Under that
discipline the alias observes a stable snapshot.
