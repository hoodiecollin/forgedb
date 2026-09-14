Read-only, positional view over a [`Tombstones`] file, created by [`Tombstones::reader`].

Same concurrency model as [`FixedColumnReader`]: an independent descriptor, positional reads, length derived from the file on every call, no cached bound.
