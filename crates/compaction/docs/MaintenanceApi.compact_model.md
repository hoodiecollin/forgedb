DEPRECATED — unsafe against the #66 mutation surface (see #105).

Manually trigger the tombstone-based offline compaction for a specific
model.  This reclaims nothing from superseding-version updates and
RESURRECTS deleted rows.  Prefer the keep-set primitive
[`Compactor::compact_model_keeping`], which the generated in-process
`Database::compact()` (#92) drives.  Retained (not `#[deprecated]`) only
to avoid a breaking change to the published `forgedb-compaction 0.1.0`
API; the offline `forgedb compact` CLI no longer calls it.
