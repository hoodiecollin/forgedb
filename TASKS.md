# ForgeDB — Backlog

A living list of potential features and known work. Not a roadmap or commitment; ideas
are grouped by scope and theme, not scheduled. Add ideas freely; remove them when done or
abandoned (git history is the record).

## Near-term cleanup (revival)

Concrete, in-flight work from the current revival pass:

- **Fix ~21 stale doctests** across 7 crates (crud-api, http-server, migrations, parser,
  query-optimization, validation, watcher) — doc-comment examples reference drifted or
  removed APIs, so plain `cargo test --workspace` fails on `--doc` targets. Fix each
  during that crate's review; some (`MigrationExecutor::new`, `SchemaDiffer::new`) may be
  a lost constructor rather than a stale doc — verify.
- **Resolve 9 dead-code warnings** — not cruft: unwired-but-live CLI flags
  (`build --no-api/--no-db`, `init --typescript`, `validate --implementations/--components`),
  an unused error exit-code scheme, an unwired `rust_main_template`, and populated-but-unread
  LSP fields. Each needs a wire-vs-remove decision (don't blindly delete).

## Feature ideas

Potential directions, unprioritized. Each would need a design note before implementation.

### Runtimes & distribution
- **WASM** — compile the engine to WebAssembly; browser storage over IndexedDB; offline-first.
- **Python bindings** (FFI via ctypes/cffi) — type-hinted API; asyncio; NumPy/Pandas interop.
- **Node.js addon** (NAPI-RS) — native addon with auto-generated TypeScript defs.
- **Deno bindings** (FFI via `Deno.dlopen`) — leverages the existing Bun FFI work.

### Storage & data
- **Distributed / replication** — WAL replication, read replicas, failover. (Optional
  consensus/Merkle verification is a separate, heavier line of exploration.)
- **Zero-knowledge storage** — client-side E2E encryption; server stores encrypted blobs;
  field- and model-level `@encrypted`.
- **MVCC concurrency** — snapshot isolation; concurrent readers/writers; version GC.
- **Backup & restore** — hot backup, point-in-time recovery, incremental, cloud targets.
- **Time-series optimization** — time partitioning, retention, downsampling.
- **Multi-tenancy** — tenant isolation, row-level security, per-tenant schemas.

### Indexing & query
- **Advanced indexes** — spatial (R-tree), vector/HNSW for semantic search, richer composite/covering indexes.
- **GraphQL** — generate a GraphQL schema + resolvers; DataLoader for N+1.
- **Real-time subscriptions** — WebSocket live queries and change notifications.

### Tooling & DX
- **Database inspection tool** (Tauri) — schema explorer, data viewer/editor, query builder,
  relation graph, performance views.
- **AI-powered development** — `@ai-implement` directive to generate components/tests from
  schema annotations. (Note the maintainability tradeoffs of generated-then-edited code.)

## Deferred

- **Restore OpenAPI generation** (deferred until after the revival refactor). The
  generator was lost during crate extraction; the CLI `openapi` target and the
  `generate all` path stay disabled/stubbed for now. Restore = re-implement as a module
  in `crates/codegen` and re-enable both call sites.

## Notes

- This is a living document; community feedback drives what actually gets built.
- Not all ideas will be implemented; favor depth on existing features over breadth.
