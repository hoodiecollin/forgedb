The **app's** schema-migration serial: how many migrations from its own
`migrations/` lineage have been applied. Derived by
`MigrationLineage::current_schema_version`, baked into generated code as
`EXPECTED_SCHEMA_VERSION`, and compared by the generated `open()` guard,
which refuses a dir whose schema is not the one this binary was generated
from (#74 Phase 1). Lineage-sourced, never hand-edited.

**The on-disk key stays `format_version` (#254).** Only the Rust name
changed, because the old one described the engine rather than the app.
A *different* vestigial `schema_version` key — hardcoded to `1` at every
write site, never incremented, never compared — used to occupy this name
on disk; it is deleted, and serde's unknown-key tolerance is what lets a
manifest still carrying it deserialize. Reading the wrong one of those two
is not a subtle bug: it would compare the open-guard against a constant.
