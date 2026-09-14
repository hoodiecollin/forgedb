A [`Compactor`] and a [`StatsCollector`] over one data directory, behind one set of methods.

Every method delegates to one of the two. The compaction methods route through the
deprecated tombstone-based [`Compactor::compact_model`]; to reclaim space from a generated
database, call [`Compactor::compact_model_keeping`] directly or the generated
`Database::compact()`. The `forgedb` CLI uses this type for `stats` and `analyze` only;
`forgedb compact` and `forgedb vacuum` refuse with an error before reaching it.
