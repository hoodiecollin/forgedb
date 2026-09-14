Compacts a model by dropping every row whose tombstone byte is set. Deprecated; use
[`Self::compact_model_keeping`].

A generated database records a delete as a tombstoned marker row and an update as a newly
appended version, so this method reclaims nothing from updates and, by dropping only the
marker, brings deleted rows back. It is not annotated `#[deprecated]`, so the compiler does
not warn. [`Self::compact_all`], [`Self::compact_needed`] and [`crate::BackgroundCompactor`]
still call it.

Reads the row count from the size of `tombstones.bin`, marks each row with a nonzero byte
for removal, and runs the same staged rewrite as [`Self::compact_model_keeping`]. Returns
`Err` when the model directory or its `tombstones.bin` is missing, or when any read, write
or rename fails.
