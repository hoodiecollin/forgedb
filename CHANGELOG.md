# Changelog

All notable changes to the ForgeDB core (the `forgedb` CLI and its published
substrate crates) are documented here. This file is generated from conventional
commits with [git-cliff](https://git-cliff.org); do not edit it by hand — run
`make changelog`. The format follows [Keep a Changelog](https://keepachangelog.com),
and the project honors [Semantic Versioning](https://semver.org) per `docs/SEMVER.md`.

## [0.3.0] - 2026-07-30

### Features

- **config:** Bake transaction max_retries as a Tier-B knob ([#146](https://github.com/hoodiecollin/forgedb/issues/146))
- **config:** Make list-endpoint pagination bounds configurable ([#141](https://github.com/hoodiecollin/forgedb/issues/141))
- **config:** Gate the /metrics endpoint as a Tier-A knob ([#151](https://github.com/hoodiecollin/forgedb/issues/151))
- **config:** Make wasm replica commit debounce configurable ([#148](https://github.com/hoodiecollin/forgedb/issues/148))
- **config:** Scaffold shutdown-drain timeout ([#142](https://github.com/hoodiecollin/forgedb/issues/142)) + multi-alg JWT allowlist ([#147](https://github.com/hoodiecollin/forgedb/issues/147))
- **config:** Wire durable replication-log retention ([#137](https://github.com/hoodiecollin/forgedb/issues/137))
- **go:** De-experimentalize the Go binding ([#204](https://github.com/hoodiecollin/forgedb/issues/204))
- **coordinator:** Configurable turn-timeout ([#144](https://github.com/hoodiecollin/forgedb/issues/144)) + max-frame ([#145](https://github.com/hoodiecollin/forgedb/issues/145))
- **auth:** JWKS-over-HTTP fetch + refresh + key rotation ([#81](https://github.com/hoodiecollin/forgedb/issues/81))

## [0.2.1] - 2026-07-30

### Bug Fixes

- **parser:** Add missing Decimal arm to FieldType::size_in_bytes ([#189](https://github.com/hoodiecollin/forgedb/issues/189))
- **init:** Refuse to start unauthenticated when FORGEDB_JWKS_URL is set ([#195](https://github.com/hoodiecollin/forgedb/issues/195))

## [0.2.0] - 2026-07-28

### Breaking changes

- **wal:** **breaking:** Opaque Raw record path; remove structured/transaction API

### Features

- **storage:** &self positional reads, explicit flush, type-aware paths
- **types:** Add Value::U32 and Value::U64 variants
- **codegen:** Generate real REST handlers with 404/201 semantics
- **cli:** Compile-time config (forgedb.toml) and distinct exit codes
- **cli:** Wire build --no-api and validate --components; fix scaffold; drop dead flags
- **lsp-server:** Struct-aware completion/hover/goto; drop dead Document fields
- **examples:** 18-app schema corpus + forgedb-schema-author agent
- **codegen:** Generate relation traversal helpers
- **parser:** Lex string-literal directive arguments
- **storage:** Add layout-manifest metadata + save_to/load_from ([#57](https://github.com/hoodiecollin/forgedb/issues/57))
- **codegen:** Emit per-model layout manifest from generated storage ([#57](https://github.com/hoodiecollin/forgedb/issues/57))
- **backup:** Add forgedb-backup snapshot create/restore crate ([#57](https://github.com/hoodiecollin/forgedb/issues/57))
- **cli:** Add `forgedb backup {create,restore,list}` ([#57](https://github.com/hoodiecollin/forgedb/issues/57))
- **storage:** Add Snapshot watermark type for lock-free read isolation ([#56](https://github.com/hoodiecollin/forgedb/issues/56))
- **codegen:** Generate watermark snapshot reads ([#56](https://github.com/hoodiecollin/forgedb/issues/56))
- **changefeed:** Add forgedb-changefeed broadcast substrate crate ([#62](https://github.com/hoodiecollin/forgedb/issues/62))
- **codegen:** Generate change-feed emits + WebSocket subscriptions ([#62](https://github.com/hoodiecollin/forgedb/issues/62))
- **cli:** Forward WebSocket Upgrade + pin changefeed in scaffold ([#62](https://github.com/hoodiecollin/forgedb/issues/62))
- **changefeed:** Add Updated/Deleted ChangeKind variants ([#66](https://github.com/hoodiecollin/forgedb/issues/66))
- **codegen:** Generate superseding-version update/delete ([#66](https://github.com/hoodiecollin/forgedb/issues/66))
- **codegen:** Stream Updated/Deleted typed WS events ([#66](https://github.com/hoodiecollin/forgedb/issues/66))
- **storage:** Read-only column reader handles for single-writer/many-reader (#56-B)
- **codegen:** Generate DatabaseReader + per-model reader handles (#56-B)
- **codegen:** Generate live-query WebSocket handler + deltas (#62-B)
- **inspector:** Scaffold Next.js + shadcn foundation
- **inspector:** Domain layer — types, mock db, jotai atoms
- **inspector:** Unified four-screen shell UI
- **inspector:** Tauri v2 desktop shell + at-rest/live backend
- **inspector:** Data-source seam + generated-API live client
- **inspector:** Wire screens to the Structure and Live lenses
- **auth:** Add forgedb-auth verify-only JWT + tenant guard substrate
- **codegen:** Multi-tenancy — root-threaded open_at + tenant-auth router
- **cli:** Forgedb tenant command, [tenant]/[auth] config, multi-tenant scaffold
- **codegen:** Generated REST update/delete endpoints ([#69](https://github.com/hoodiecollin/forgedb/issues/69))
- **inspector:** Relation-graph view in Atlas ([#70](https://github.com/hoodiecollin/forgedb/issues/70))
- **inspector:** Live create/replace/delete + editable API base (#68, #71)
- **query-optimization:** Implement join predicate pushdown ([#48](https://github.com/hoodiecollin/forgedb/issues/48))
- **codegen:** Restore OpenAPI generation ([#49](https://github.com/hoodiecollin/forgedb/issues/49))
- **storage:** Truncate_to_rows + DirLock for durable writes (0.1.5)
- **codegen:** Wire durable write path into generated writes ([#89](https://github.com/hoodiecollin/forgedb/issues/89))
- **codegen:** Bound the WAL with a generated checkpoint ([#96](https://github.com/hoodiecollin/forgedb/issues/96))
- **codegen:** Real list endpoint with filter/sort/paginate ([#90](https://github.com/hoodiecollin/forgedb/issues/90))
- **codegen:** Secondary indexes + find_by_/get_by_ probes ([#90](https://github.com/hoodiecollin/forgedb/issues/90))
- **cli:** Pin forgedb-query-params in the init scaffold ([#90](https://github.com/hoodiecollin/forgedb/issues/90))
- **codegen:** Phase-2 index follow-ups #100–#103
- **codegen:** Enforce data integrity at write (#91 Phase 3)
- **codegen:** In-process auto-compaction + additive backfill (#92 W1/W2)
- **cli:** Additive-vs-breaking migrate --auto gate (#92 W3)
- **codegen:** Add observability endpoints + request logging to generated API
- **cli:** Scaffold structured logging, graceful shutdown, and Docker deploy path
- **codegen:** Complete the generated TypeScript SDK
- **changefeed:** Durable resumable replication broker ([#82](https://github.com/hoodiecollin/forgedb/issues/82))
- **codegen:** Wire durable broker + /replicate endpoint ([#82](https://github.com/hoodiecollin/forgedb/issues/82))
- **types:** Add wasm32 uuid js feature for browser builds
- **codegen:** Generate follower apply_frame + commit for read-replica
- **codegen:** Generate the wasm browser-replica transport per-schema
- **cli:** Add `generate wasm` target for the browser replica crate
- **storage-web:** OPFS per-column files via sync-access-handles in a Worker
- **storage-web:** Engine-in-Worker lazy per-column fault-in + incremental commit
- **parser:** Add @projection directive + Projection AST
- **codegen:** Column projection reads + narrow-read decode path
- **codegen:** Enforce @pattern/@regex field validation ([#104](https://github.com/hoodiecollin/forgedb/issues/104))
- Add json scalar type to the schema language
- Add decimal scalar type (rust_decimal, exact fixed-point)
- Add user-declared enum types
- Delete semantics — @on_delete restrict/cascade/set_null + M2M unlink
- **codegen:** Compaction-epoch lifecycle + recover_to PITR replay
- **backup:** Incremental chain + PITR broker-offset watermark
- **cli:** Backup incremental / chain-restore / list ([#76](https://github.com/hoodiecollin/forgedb/issues/76))
- **wal:** Add WalManager::truncate_to for partial WAL-tail rollback
- **codegen:** MVCC Tier 1 transactions — generated TxHandle + commit journal ([#75](https://github.com/hoodiecollin/forgedb/issues/75))
- **txn:** Add forgedb-txn commit sequencer substrate ([#75](https://github.com/hoodiecollin/forgedb/issues/75))
- **codegen:** MVCC Tier 2 genuine concurrent prepare ([#75](https://github.com/hoodiecollin/forgedb/issues/75))
- **coordinator:** Add forgedb-coordinator control-plane substrate ([#75](https://github.com/hoodiecollin/forgedb/issues/75))
- **cli:** Add `forgedb coordinate <root>` subcommand ([#75](https://github.com/hoodiecollin/forgedb/issues/75))
- **codegen:** MVCC Tier 3 coordinated-client glue — PARTIAL ([#75](https://github.com/hoodiecollin/forgedb/issues/75))
- **storage-native:** Sync_from_disk on all column types for Tier 3 peer refresh ([#84](https://github.com/hoodiecollin/forgedb/issues/84))
- **coordinator:** Hold the #89 DirLock so standalone writers are excluded (#84, T3-5)
- **codegen:** Tier 3 data plane — lock-free coordinated open + all-column peer refresh ([#84](https://github.com/hoodiecollin/forgedb/issues/84))
- **storage-web:** No-op sync_from_disk for wasm API parity + bump 0.1.1 ([#84](https://github.com/hoodiecollin/forgedb/issues/84))
- **codegen:** Derive PartialEq on generated structs
- **codegen:** Typed per-field event filter + live-query change detector ([#84](https://github.com/hoodiecollin/forgedb/issues/84))
- **codegen:** Point-in-time snapshot reads over REST ([#85](https://github.com/hoodiecollin/forgedb/issues/85))
- **inspector:** Wire the snapshot scrubber to real point-in-time reads ([#85](https://github.com/hoodiecollin/forgedb/issues/85))
- **init:** Emit on-host systemd deploy artifacts ([#115](https://github.com/hoodiecollin/forgedb/issues/115))
- **migrations:** Hop classification + serial version lineage, drop executor ([#74](https://github.com/hoodiecollin/forgedb/issues/74))
- **codegen:** Format-version guard + offline transformer generator ([#74](https://github.com/hoodiecollin/forgedb/issues/74))
- **cli:** Migrate build/run/up lifecycle + generate transform ([#74](https://github.com/hoodiecollin/forgedb/issues/74))
- **storage:** Add class-1 gather primitive for columnar export
- **codegen:** Add FfiGenerator — the L0 C-ABI spine
- **cli:** Wire `generate ffi` native engine target
- **codegen:** FFI per-model row ops (bindings Phase 3)
- **codegen:** FFI relation-traversal getters (bindings Phase 3)
- **codegen:** FFI snapshot _at reads (bindings Phase 3)
- **codegen:** FFI async completion bridges (bindings Phase 3)
- **codegen:** Generated per-model columnar-gather methods
- **codegen:** FFI Arrow columnar export ops (the zero-copy selling point)
- **storage:** ColumnExport + mmap-alias export primitive (dense_prefix)
- **codegen:** Generated columnar export takes the mmap-alias fast path
- **codegen:** FFI Arrow export carries the alias-or-gather ColumnExport
- **cli:** Restructure generate into runtime × mode axes ([#122](https://github.com/hoodiecollin/forgedb/issues/122))
- **codegen:** PyO3 Python binding generator ([#51](https://github.com/hoodiecollin/forgedb/issues/51))
- **cli:** Wire generate python --runtime to the PyO3 binding ([#51](https://github.com/hoodiecollin/forgedb/issues/51))
- **codegen:** NAPI-RS Node/Bun binding generator
- **cli:** Wire generate node|bun --runtime to the NAPI-RS binding
- **codegen:** PyO3 relation traversal (5b)
- **codegen:** NAPI-RS relation traversal (6b)
- **codegen:** PyO3 Arrow columnar export (5b)
- **codegen:** NAPI-RS Arrow columnar export (6b)
- **codegen:** Typed row structs in NAPI and PyO3 wrappers
- **codegen:** Async NAPI CRUD over a shared RwLock engine handle
- **benchmarks:** Add ForgeDB-vs-SQLite benchmark harness (first cut)
- **benchmarks:** Measure SQLite inserts at both durability levels
- **codegen:** Add GenConfig for generate-time runtime-behavior knobs ([#126](https://github.com/hoodiecollin/forgedb/issues/126))
- **cli:** Wire [runtime]/[storage] config into generate/build ([#127](https://github.com/hoodiecollin/forgedb/issues/127))
- **storage:** Add sync_to_drive/barrier coalesced-durability primitives ([#153](https://github.com/hoodiecollin/forgedb/issues/153))
- **cli:** Forgedb coordinate --fsync always|never|periodic ([#156](https://github.com/hoodiecollin/forgedb/issues/156))
- **codegen:** #162 defer auto-compaction off the write turn + in-place remap
- **codegen:** #161-B incremental delta peer refresh
- **storage:** Bulk-buffered column read primitives ([#168](https://github.com/hoodiecollin/forgedb/issues/168))
- **wal:** Buffered (no-fsync) append for group commit ([#170](https://github.com/hoodiecollin/forgedb/issues/170))
- **brand:** Add ForgeDB brand kit assets
- **parser:** Carry source positions on AST nodes (#175 WS2 2a)
- **parser:** Consolidate schema validation into positioned validate_schema (#173 WS2b)
- **parser:** Resilient parse_recover with two-tier error recovery (#173 WS2c)
- **lsp:** Re-point language server onto the compiler (#173 WS2d)
- **lsp:** Expose a library target for the diagnostic mapper (#173 WS3)
- **cli:** Add a `forgedb lsp` launcher subcommand (#173 WS4)
- **cli:** Bundle forgedb-lsp behind the root crate's `lsp` feature
- **cli:** Add verbosity levels to ui and wire -v/-q
- **generate:** Implement --check staleness gate
- **codegen:** Auto-generate + uuid/timestamp fields on create (#187, #188)
- **codegen:** Add experimental Golang binding generator
- **cli:** Wire `generate go --runtime` experimental target
- **codegen:** Go binding NAPI-parity features in the generator
- **cli:** Emit forgedb_async.go + README for the Go binding
- **codegen:** Arrow columnar export for the Go binding
- **cli:** Emit forgedb_arrow.go + arrow-go dep notice
- **codegen:** Add creatable-fields helper for REST SDKs
- **codegen:** Rust/Python/Go REST client SDK generators
- **cli:** Wire rust|python|go --sdk into generate
- **dist:** Add MSI/WiX Windows installer channel
- **dist:** Add winget release workflow
- **dist:** Add Docker image + multi-arch publish workflow
- **dist:** Add Nix flake

### Bug Fixes

- **examples:** Update query-params examples to current API
- **examples:** Port http-server middleware example to axum 0.8
- **wal:** Harden corruption paths and transaction recovery
- **storage:** Guard value_size, atomic manifest write, is_empty
- **types:** Saturate Timestamp::now and add must_use
- **codegen:** Emit generated Rust that actually compiles
- **parser:** Error on numeric overflow, support nullable primitives
- **migrations:** Rename detection, deterministic diffs, honest executor
- **validation:** Correct stale doctests to real API
- **compaction:** Manifest-driven counts, atomic staging, storage-matching tombstones
- **query-optimization:** Preserve join predicates and fix unsigned scan
- **query-params:** Correct filter coercion and pagination overflow
- **http-server:** Close auth bypass and rate-limit spoofing, harden defaults
- **watcher:** Wire real codegen regeneration and robust path matching
- **crud-api:** Correct stale doctests to real types
- **cli:** Idempotent build, loud migrate --auto, safe serve, scaffold deps
- **lsp-server:** Correct modifier semantics and safe references
- **codegen:** Persist FK scalars and emit compile-clean database.rs
- **http-server:** Repair broken config examples
- **codegen:** Compile-clean generated code for the examples corpus
- **codegen:** Rehydrate storage index on reopen so data survives restart ([#65](https://github.com/hoodiecollin/forgedb/issues/65))
- **compaction:** Derive row count from tombstone length, not a manifest ([#65](https://github.com/hoodiecollin/forgedb/issues/65))
- **codegen:** Emit OpenAPI 3.1.0 to match the runtime utoipa path ([#49](https://github.com/hoodiecollin/forgedb/issues/49))
- **compaction:** Reclaim generated variable columns + add keep-set primitive
- **cli:** Guard offline compact/vacuum against #66 resurrection ([#105](https://github.com/hoodiecollin/forgedb/issues/105))
- **cli:** Deprecate unsafe offline compact/vacuum; point to in-process Database::compact ([#105](https://github.com/hoodiecollin/forgedb/issues/105))
- **init:** Scaffold pins forgedb-storage 0.2 facade so MVCC code resolves (#75, #84)
- **codegen:** Cfg-gate Tier-3 coordinator surface off wasm + add forgedb-txn to replica scaffold (#75, #84)
- **codegen:** Pin napi row-struct JS keys to snake_case field names
- **compaction:** Guard divide-by-zero when compacting an empty model
- **examples:** Make vscode example.forge parse and use @length
- **parser:** Record token start after whitespace skip
- **codegen:** Bump generated forgedb-coordinator pin to 0.2
- **init:** Correct scaffold hint to `forgedb generate rust`

### Performance

- **codegen:** Index M2M traversal ([#154](https://github.com/hoodiecollin/forgedb/issues/154)) + single-barrier checkpoint ([#153](https://github.com/hoodiecollin/forgedb/issues/153))
- **coordinator:** Move broker append+fsync off the turn critical section ([#156](https://github.com/hoodiecollin/forgedb/issues/156))
- **codegen:** Serialize the record once per write, shared WAL+broker (#157 part A)
- **codegen:** Cache per-write index-key derivation (#157 part B)
- **codegen:** Share reader index maps via Arc + copy-on-write ([#158](https://github.com/hoodiecollin/forgedb/issues/158))
- **codegen:** Resolve snapshot reads via per-id version index ([#159](https://github.com/hoodiecollin/forgedb/issues/159))
- **codegen:** Narrow list scan + index pushdown for the live list path ([#160](https://github.com/hoodiecollin/forgedb/issues/160))
- **codegen:** Column-pruned sequential __scan ([#168](https://github.com/hoodiecollin/forgedb/issues/168))
- **codegen:** Ordered/range index kind ([#169](https://github.com/hoodiecollin/forgedb/issues/169))
- **codegen:** Narrow live-query re-run materialization ([#160](https://github.com/hoodiecollin/forgedb/issues/160))
- **codegen:** Group commit — buffered staging, one barrier per txn ([#170](https://github.com/hoodiecollin/forgedb/issues/170))
- **codegen:** Column-pruned projected buffered scan (#167/#168)
- **benchmarks:** Projected scan_aggregate + fair bulk-load framing

