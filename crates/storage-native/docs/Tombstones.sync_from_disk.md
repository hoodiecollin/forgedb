Re-read the on-disk tombstone count from the file metadata and update the
cached `count`.

Normally `count` is maintained in memory by `append`/`truncate_to_rows`, so
this is a no-op for the single-writer path.  For the Tier 3 multi-process
writer path (#84) a peer process may have appended tombstone bytes since this
writer opened the file; calling `sync_from_disk` before `len()` lets the
writer see the peer's committed rows without reopening the storage.

# Errors

Propagates any error from `file.metadata()`.
