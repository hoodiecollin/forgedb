The app's schema-migration serial: how many migrations from the app's own lineage have been applied to this directory.

Generated code bakes the expected serial in as a constant and refuses to open a directory whose serial differs, because the bytes on disk would then belong to a different schema than the binary was generated from. Missing from the file means `1`.

The on-disk key is `format_version`, not `schema_version`. A `schema_version` key written by an older engine may still be present in the file; it is ignored.
