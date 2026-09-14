Deletion flags for a model: one byte per row in a single file, `0` for live and `1` for deleted.

A delete flips a byte rather than moving data, so row indices stay stable. Reads take `&self` and use positional I/O; appends take `&mut self`. The count is maintained in memory and refreshed from the file only by [`Tombstones::sync_from_disk`]. Generated code appends this file last for each inserted row, which is what makes its length the committed row count (see [`Manifest::row_anchor`]).

# Durability

Appends write to the OS page cache and do **not** fsync; a crash before [`Tombstones::flush`] (or `sync_to_drive` followed by `barrier`) can lose the most recent appends.
