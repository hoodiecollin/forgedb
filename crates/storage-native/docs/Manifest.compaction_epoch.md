Generation counter bumped on every compaction. A byte-watermark
incremental backup (#57) is valid only within one epoch — compaction
rewrites files and shifts offsets, so crossing an epoch forces a fresh
full backup. Additive (`#[serde(default)]`) for on-disk back-compat.
