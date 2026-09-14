ForgeDB's own **engine byte-format generation** (#254) — owned by the
released version line, not by the app. It covers both value
reinterpretation and physical layout, which is why it is not
`layout_version`.

Orthogonal to `schema_version` above: a schema mismatch means *the app's
schema changed* and is fixed by the app's migration bin; an engine
mismatch means *ForgeDB changed* and is fixed by `forgedb migrate engine`.
Conflating them would send a user to regenerate a schema that is correct.

Generations are assigned at merge order, not at design time — two format
changes in one cycle would otherwise both claim the same number and
whichever landed second would silently redefine the other's meaning.

| gen | change |
|---|---|
| 1 | baseline — everything written before this field existed |
| 2 | timestamp values are microseconds, not seconds (#254) |
