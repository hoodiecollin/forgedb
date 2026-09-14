Generation counter the writer increments on every compaction. Missing from the file means `0`.

Compaction rewrites files and shifts offsets, so a byte-watermark incremental backup is valid only within one epoch; the backup tooling compares this value to decide whether a fresh full snapshot is needed.
