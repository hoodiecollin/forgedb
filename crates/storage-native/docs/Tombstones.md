Tombstone bitmap for tracking deleted records.

# Durability

Append methods write to the OS page cache but do **not** call `fsync` per append.
Call [`flush`](Tombstones::flush) at commit boundaries to guarantee durability.
