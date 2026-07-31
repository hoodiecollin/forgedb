# ForgeDB Substrate Crates

**Audience:** developers of generated ForgeDB apps, and anyone consuming the published
`forgedb-*` runtime crates.

Generated ForgeDB code is **not** dependency-free. It links a small set of **schema-agnostic
substrate crates** published to crates.io. This document catalogs them. For the *stability
policy* over these crates (semver, on-disk format, the compiler-internals carve-out) see
[SEMVER.md](./SEMVER.md); for the version matrix a given CLI pins see [INSTALL.md](./INSTALL.md);
the authoritative in-repo inventory is the "Workspace layout" section of
[`CLAUDE.md`](../CLAUDE.md).

---

## What "substrate" means

ForgeDB is an **application-database generator**, not a runtime ORM. The schema-specific
surface — types, tables, queries, filters, relations, API routes — is *generated per app at
compile time*. The substrate crates are the schema-agnostic layer the generated code links
against: they know nothing about any particular schema.

Two classes ship, both fine under the generator-identity rule (see `CLAUDE.md`):

1. **Schema-agnostic substrate** the generated code links (storage, wal, types, changefeed,
   auth, query-params, compaction, txn, coordinator). Real programmatic APIs that interpret no
   schema.
2. **Access/transport glue** over the already-generated surface — the language bindings
   (Python/PyO3, Node+Bun/NAPI-RS, the wasm read-replica transport, and the Golang cgo
   target, RFC #203). Not covered here — each is generated per schema, not a
   standalone crate. They ride the generated native FFI C-ABI and interpret no schema.

A generic, schema-reading query builder or ORM is explicitly **not** substrate and is never
shipped.

---

## The published substrate

Each crate is versioned on an **independent** line — the pins are intentionally *not*
normalized. Versions below are current as of this writing; treat the crate manifests and
crates.io as authoritative.

| Crate | Version | Role |
|---|---|---|
| `forgedb-types` | 0.2.1 | Core type system (uuid, timestamp, primitives). A `cfg(wasm32)` uuid/getrandom feature for the browser build. |
| `forgedb-storage` | 0.2.3 | Columnar storage **facade** — a thin `cfg` re-export selecting a backend by target. Generated code uses only this crate. |
| `forgedb-storage-native` | 0.1.4 | Native positional-file columnar backend (host targets). Selected by the facade on non-wasm. |
| `forgedb-storage-web` | 0.1.4 | In-memory-arena columnar backend for the browser read-replica (wasm32); async only at the IndexedDB/OPFS hydrate/commit boundary. Byte-identical positional semantics to native. |
| `forgedb-wal` | 0.2.3 | Write-ahead log. Generated code links the **opaque `Raw`** record path (bytes + CRC framing + fsync policy + torn-tail recovery). File impl on host, in-memory impl on wasm32. |
| `forgedb-changefeed` | 0.2.0 | Field-blind change-feed broadcast **+ durable replication broker** (`durable` module: CRC-framed append-only log at a monotonic global offset; resumable subscription). |
| `forgedb-auth` | 0.2.0 | Verify-only JWT + tenant cross-check axum extractor/middleware (static PEM, or opt-in `jwks-http` fetch + background rotation; algorithm-pinned). Injects an opaque `Principal`; knows no model/row. |
| `forgedb-query-params` | 0.1.0 | REST query-string parser (URL → generic `Filter`/`Sort`/`Pagination`, limit clamped). All field-aware filtering/sorting is generated per-model; this parses only the string. |
| `forgedb-compaction` | 0.1.0 | In-process dead-row reclaim keyed by model *directory name*. `compact_model_keeping(model, live_rows)` keeps caller-supplied opaque row indices (the generated code computes the live set). |
| `forgedb-txn` | 0.1.0 | MVCC Tier 2 commit sequencer: monotonic LSN + first-committer-wins conflict detection over an in-memory `id → last-committer` map (rebuilt empty on open; never persisted). |
| `forgedb-coordinator` | 0.2.1 | MVCC Tier 3 multi-process write control plane. A `forgedb coordinate <root>` process holds the `DirLock`, serializes the commit turn, sequences the LSN (configurable `--turn-timeout`/`--max-frame-mib`). **No `forgedb-storage*` dep** — it never writes columns. |

### Not substrate

`forgedb-parser`, `forgedb-codegen`, `forgedb-validation`, `forgedb-migrations`,
`forgedb-backup`, `forgedb-watcher`, and `forgedb-lsp-server` are also on crates.io, but
**only so that `cargo install forgedb` can build the CLI from the registry**. They are
compiler internals, explicitly **not** a stable public API — see [SEMVER.md §4](./SEMVER.md)
and [`CLAUDE.md`](../CLAUDE.md). (`forgedb-lsp-server` joined this list in epic #173,
when the `forgedb` crate gained an optional dependency on it for the bundled `forgedb-lsp`
binary; it must be published before the next `forgedb` publish.)

---

## Dependency graph (substrate only)

```
types            (leaf)
wal              (leaf)
changefeed       (leaf)
auth             (leaf)
query-params     (leaf)
compaction       (leaf)
txn              (leaf)

wal ──► storage-native ─┐
wal ──► storage-web  ───┤ cfg-selected by target
                        └─► storage (facade)

txn ─┐
     ├─► coordinator
changefeed ─┘
```

The facade never links both backends at once: `cfg(not(target_arch = "wasm32"))` pulls
`storage-native`, `cfg(target_arch = "wasm32")` pulls `storage-web`. Their (identical) public
types therefore never collide.

The on-disk column layout is part of the substrate ABI (see [SEMVER.md §2](./SEMVER.md)) and is
described in [ARCHITECTURE.md](./ARCHITECTURE.md#storage-model).

---

## How generated code links them

`forgedb init` writes a scaffold `Cargo.toml` that pins each substrate crate with a caret
range, e.g.:

```toml
[dependencies]
forgedb-types        = "0.2"
forgedb-storage      = "0.2"
forgedb-wal          = "0.2"
forgedb-changefeed   = "0.2"
forgedb-auth         = "0.2"
forgedb-query-params = "0.1"
forgedb-compaction   = "0.1"
forgedb-txn          = "0.1"
forgedb-coordinator  = "0.2"
```

The generated `database.rs` uses the storage facade verbatim
(`use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};`) and stays byte-identical
across native and wasm targets — the facade absorbs the target difference. The exact pins the
current CLI emits live in [INSTALL.md](./INSTALL.md); `forgedb init` always pins versions the
CLI is known-compatible with.

**Publish discipline:** whenever generated code starts requiring a new substrate crate or a new
additive substrate API, that crate/version must publish *before* the scaffold pins it, or an
outside-repo `init → generate → cargo build` cannot resolve from crates.io. This "publish gap"
discipline is described in [PUBLISHING.md](./PUBLISHING.md).

---

## References

- [ARCHITECTURE.md](./ARCHITECTURE.md) — system architecture and the storage model
- [SEMVER.md](./SEMVER.md) — stability policy across the four surfaces
- [INSTALL.md](./INSTALL.md) — install paths + substrate version matrix
- [PUBLISHING.md](./PUBLISHING.md) — the release/publish runbook
- [`CLAUDE.md`](../CLAUDE.md) — authoritative workspace inventory
