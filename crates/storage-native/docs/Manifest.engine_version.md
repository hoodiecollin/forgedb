ForgeDB's own byte-format generation, owned by the released engine rather than by the app's schema.

It is orthogonal to [`Manifest::schema_version`]: a schema mismatch means the app's schema changed and is fixed by the app's migration; an engine mismatch means ForgeDB changed how bytes are laid out or interpreted and is fixed by `forgedb migrate engine`. Generated code refuses to open a directory whose generation differs from the one it was built for. Missing from the file means `1`.

| generation | change |
|---|---|
| 1 | baseline: everything written before this field existed |
| 2 | timestamp values are stored in microseconds rather than seconds |
