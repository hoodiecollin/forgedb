On-disk layout format version, so a schema-blind reader (backup #57,
inspector #63) can refuse mismatched bytes instead of misreading them.
Which file's length authoritatively counts committed rows, and how many
bytes it spends per row. For a model this is `tombstones.bin` (1 byte/row,
appended last per insert); for an M2M junction it is `fixed/right.bin`
(16 bytes/row, appended last per link). A schema-blind reader derives the
committed row count as `len(anchor) / bytes_per_row` — the live watermark,
independent of the (possibly stale) `row_count` field above. `None` on
legacy manifests ⇒ fall back to `tombstones.bin`.
