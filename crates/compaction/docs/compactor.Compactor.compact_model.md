Compact a specific model (tombstone-based, **DEPRECATED** — see #105).

Unsafe against the #66 generated mutation surface: it reclaims nothing
from superseding-version updates and RESURRECTS deleted rows (a delete
tombstones a *marker* row, not the old data row).  Use
[`Compactor::compact_model_keeping`] — the keep-set primitive the
generated in-process `Database::compact()` (#92) drives.  Retained (not
`#[deprecated]`) only to keep the published `forgedb-compaction 0.1.0`
API stable; the offline `forgedb compact` CLI no longer calls it.

# Crash safety (C2)

All compacted column files are written as `.tmp` siblings first.  Only
after every write succeeds are they renamed to their final paths.  The
manifest `row_count` is updated last so it acts as a logical commit
record: if the process dies before the manifest rename, the column files
are already consistent and a re-run is idempotent.

**Residual window**: individual file renames are atomic but not grouped
— a crash between two column-file renames leaves those columns at
different post-compaction versions.  Full directory-level atomicity is
deferred.
