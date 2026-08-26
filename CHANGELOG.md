# Changelog

All notable changes to the ForgeDB core (the `forgedb` CLI and its published
substrate crates) are documented here. This file is generated from conventional
commits with [git-cliff](https://git-cliff.org); do not edit it by hand — run
`make changelog`. The format follows [Keep a Changelog](https://keepachangelog.com),
and the project honors [Semantic Versioning](https://semver.org) per `docs/SEMVER.md`.

## [0.5.0] - 2026-08-26

> **Breaking release — upgrading needs action.** Required migration steps are in
> [docs/UPGRADING.md](https://github.com/hoodiecollin/forgedb/blob/main/docs/UPGRADING.md).

### Breaking changes

- **config:** **breaking:** Require [generate].targets, and speak the CLI's vocabulary
- **naming:** **breaking:** Derive every name from the schema's path, not from a hash
- **config:** **breaking:** [project].symbol_naming, and the targets doc it owed
- **build:** **breaking:** A cargo driver owns every compile, and refuses collisions first
- **migrate:** **breaking:** --schema is required, and there is no fallback
- **init:** **breaking:** Scaffold no cargo package at all
- **build:** **breaking:** The Tauri inspector is its own workspace, not a root member
- **codegen:** **breaking:** Floor a timestamp probe to the field quantum, like the write
- **migrations:** **breaking:** A specified checksum, tagged with its own name
- **generate:** **breaking:** Read the migration lineage from the schema's directory, not the CWD

### Features

- **cache:** The ForgeDB-owned build cache dir and its workspace root
- **config:** Read [project], reject unknown keys, remove [generate].schema
- **project:** Resolve which project a schema belongs to
- **cli:** Resolve config by walking up from the schema
- **cache:** Place each app in its project's cache workspace
- **naming:** Derive every artifact name, so no two apps can collide
- **cache:** Apps/<hash> becomes a container, members are derived by scanning
- **cache:** Drop Cargo.lock when the CLI version changes (C9)
- **codegen:** Gate the OpenAPI surface behind a GenConfig.web knob
- **codegen:** Emit the core/ and server/ cache packages
- **cache:** Reserve before emission, sync the root after
- **cache:** Record the derived app name, and wire the per-kind prune
- **codegen:** The four wrappers link core, behind a per-app symbol prefix
- **codegen:** Range-stamp the transform and engine members
- **dev:** Route dev through generate, so it stops emitting defaults
- **cli:** Govern -> identify -> reserve -> emit -> sync, on every command
- **config:** The [placement] table, and its one reader
- **generate:** Emit the in-tree Rust package, and print the line that links it
- **generate:** Refuse a placement inside the cache, and check it without writing
- **project:** An askable boundary, and the vocabulary of a decision ([#367](https://github.com/hoodiecollin/forgedb/issues/367))
- **ask:** The terminal widget, last and smallest ([#367](https://github.com/hoodiecollin/forgedb/issues/367))
- **fingerprint:** One FNV, and a source fingerprint that excludes itself
- **codegen:** Both halves of the load check, and the header moves to its definitions
- **generate:** Plan packages before writing them, and emit the consumer half
- **build:** One delivery table, total over PackageKind
- **migrations:** Widen the provable set, and make both classifiers exhaustive
- **migrations:** Record the answer as data, beside the change
- **migrate:** Refuse an unanswered hop, and lower the answer into the ops
- **cli:** The askability boundary and two prompt widgets
- **migrate:** Create detects by default, and asks what it cannot prove
- **migrations:** A rename is proposed, never assumed
- **config:** [toolchain] — where the interpreters ForgeDB links to live
- **codegen:** Escape to the author's own runtime, over baked NDJSON
- **project:** Mint the id at init instead of deriving it

### Bug Fixes

- **cli:** Build honors --config for the [runtime]/[storage] knobs
- **tests:** Stop config_flag_test doing a real release build ([#380](https://github.com/hoodiecollin/forgedb/issues/380))
- **tests:** Stop scenario_15 doing a real release build ([#380](https://github.com/hoodiecollin/forgedb/issues/380))
- **tests:** Resolve scenario_3's staticlibs through cargo, not a hash substring ([#386](https://github.com/hoodiecollin/forgedb/issues/386))
- **tests:** Parse `ldd` as `ldd`, not as `otool -L`
- **codegen:** Make GenConfig::needs_utoipa the one utoipa condition
- **generate:** Render core/Cargo.toml from the config that generated core
- **migrations:** Classify an enum/struct definition change positionally
- **migrations:** Let the differ see enum and struct definitions
- **migrate:** Project enums, structs and their transitive deps into the diff
- **ask:** Restore two mangled prompt strings, and assert the rendering ([#367](https://github.com/hoodiecollin/forgedb/issues/367))
- **tests:** Bound the #170 insert-fsync guard to insert's own body
- **migrate:** Decompose nullability out of the diffed type
- **codegen:** Honour @default on BOTH routes, from one definition
- **migrate:** A hop answered in the record needs no transform.rs
- **codegen:** An optional FK is nullable once, not twice
- **generate:** Running `generate` a second time is not an error
- **ui:** A quiet stdout carries the payload and nothing else
- **codegen:** Index.d.ts declares every method the addon exports
- **tests:** The #170 fsync guard checked one model and still degraded on a miss
- **codegen:** A snapshot read returns rows in ascending physical row order
- **test:** The [project] fixtures the scoped rename missed
## [0.4.1] - 2026-08-13

### Bug Fixes

- **migrate:** Ask cargo for the transformer path instead of guessing it
## [0.4.0] - 2026-08-12

> **Breaking release — upgrading needs action.** Required migration steps are in
> [docs/UPGRADING.md](https://github.com/hoodiecollin/forgedb/blob/main/docs/UPGRADING.md).

### Breaking changes

- **types:** **breaking:** Timestamp is microseconds, and its wire form is RFC 3339
- **storage:** **breaking:** Two orthogonal version counters on the manifest
- **breaking:** Rename the schema serial, and bake an engine generation beside it
- **parser:** **breaking:** Timestamp declares its precision
- **codegen:** **breaking:** Every generated wire form is the RFC 3339 string
- **codegen:** **breaking:** Allocate a +timestamp identity, and floor what is written

### Features

- **parser:** Warn that &/^ on the identity field has no effect ([#258](https://github.com/hoodiecollin/forgedb/issues/258))
- **storage:** Carry per-field allocation high-water marks in the manifest
- **parser:** Reject integer autos that are not conflict-visible
- **codegen:** Allocate +u32/+u64 at create, seeded by scan and a persisted floor
- **codegen:** Emit a sequence claim key for bare integer autos
- **parser:** Accept a bare non-unique integer auto
- **storage:** Borrow a fixed column's slot instead of copying it ([#238](https://github.com/hoodiecollin/forgedb/issues/238))
- **parser:** `string(N)` and `string(N!)` parse to a fixed-width inline type ([#238](https://github.com/hoodiecollin/forgedb/issues/238))
- **codegen:** Generate the inline string column, end to end ([#238](https://github.com/hoodiecollin/forgedb/issues/238))
- **validation:** The semantic rules for `string(N)` ([#238](https://github.com/hoodiecollin/forgedb/issues/238))
- **codegen:** Resolve a foreign key to its target's identity type
- **codegen:** Every surface keys a relation on the target's own id
- **codegen:** A junction keys each endpoint on its own identity
- **parser:** Report an identity cycle, an inherited key width, and an unholdable junction key
- **parser:** A +timestamp identity must be named `id` and declared `us`
- **codegen:** Generate the engine-format migration hop
- **cli:** Forgedb migrate engine
- **types:** InlineStr<BYTES>, a Copy fixed-capacity string key
- **parser:** A string identity carries a declared width, and no @utf8
- **codegen:** A string identity generates an InlineStr<N> key
- **codegen:** An identity is checked against the URL alphabet at write
- **parser:** One identity predicate, and one key-type predicate, on the AST
- **parser:** The identity type allow-list, landing once
- **codegen:** Reconnect the coordinator client in both error arms ([#274](https://github.com/hoodiecollin/forgedb/issues/274))
- **codegen:** Emit a CorsLayer and check WebSocket origins ([#140](https://github.com/hoodiecollin/forgedb/issues/140))
- **cli:** Wire FORGEDB_CORS_ORIGINS through the generated scaffold ([#140](https://github.com/hoodiecollin/forgedb/issues/140))
- **codegen:** Carry the buffer slot on the scan view ([#226](https://github.com/hoodiecollin/forgedb/issues/226))
- **codegen:** Emit the borrowed full-record page view ([#226](https://github.com/hoodiecollin/forgedb/issues/226))
- **codegen:** Emit the __with_page scan-and-page scope ([#226](https://github.com/hoodiecollin/forgedb/issues/226))

### Bug Fixes

- **codegen:** Enforce &/^ on non-identity auto fields ([#258](https://github.com/hoodiecollin/forgedb/issues/258))
- **codegen:** Persist the auto-increment floor before compaction destroys the rows
- **codegen:** Parenthesize the junction frame's slot before try_into
- **codegen:** `id` wins the identity by name, not by declaration order
- **codegen:** The REST id parse resolves the identity type, not `_ => Uuid`
- **codegen:** The browser replica parses a string key
- **codegen:** An FK to a string key is an inline-string column everywhere
- **docs:** Two schema examples in SCHEMA.md that never parsed
- **coordinator:** Couple the client's I/O deadline to the grant wait ([#274](https://github.com/hoodiecollin/forgedb/issues/274))
- **codegen:** Read the page view's scan fields by value, not by no-op clone ([#226](https://github.com/hoodiecollin/forgedb/issues/226))

### Performance

- **codegen:** Serialize the live list page from the scan buffers ([#226](https://github.com/hoodiecollin/forgedb/issues/226))
- **codegen:** Hoist the unfiltered-list predicate out of the per-row loop
- **codegen:** Emit __with_fast_page, the unfiltered list read
- **codegen:** Route the unfiltered, unsorted list through the fast page
## [0.3.2] - 2026-08-06

### Features

- **storage:** Add BufferedVariableColumn::read_str for borrowed slot reads ([#224](https://github.com/hoodiecollin/forgedb/issues/224))
- **codegen:** Decode the narrow scan into a borrowed row view ([#224](https://github.com/hoodiecollin/forgedb/issues/224))
- **validation:** Add a Severity axis to ValidationError ([#237](https://github.com/hoodiecollin/forgedb/issues/237))
- **parser:** Add a warning channel alongside recovery diagnostics ([#237](https://github.com/hoodiecollin/forgedb/issues/237))
- **lsp:** Publish diagnostics at their compiler severity ([#237](https://github.com/hoodiecollin/forgedb/issues/237))
- **cli:** Partition diagnostics by severity instead of failing on any ([#237](https://github.com/hoodiecollin/forgedb/issues/237))
- **storage-native:** Additive append_tagged on VariableColumn ([#231](https://github.com/hoodiecollin/forgedb/issues/231))
- **storage-web:** The append_tagged arena twin ([#231](https://github.com/hoodiecollin/forgedb/issues/231))
- **parser:** Rename char(n) to bytes(n), warn on the old spelling ([#233](https://github.com/hoodiecollin/forgedb/issues/233))
- **parser:** Named `min:`/`max:` arguments for @length ([#235](https://github.com/hoodiecollin/forgedb/issues/235))
- **codegen:** Emit a distinct check per @length spelling ([#235](https://github.com/hoodiecollin/forgedb/issues/235))
- **parser:** Accept negative, fractional, and exclusive numeric bounds ([#239](https://github.com/hoodiecollin/forgedb/issues/239))
- **parser:** Reject bound shapes a numeric domain cannot mean ([#239](https://github.com/hoodiecollin/forgedb/issues/239))
- **codegen:** Emit exact decimal and exclusive numeric bounds ([#239](https://github.com/hoodiecollin/forgedb/issues/239))

### Bug Fixes

- **validation:** Reject a model with no identity field ([#248](https://github.com/hoodiecollin/forgedb/issues/248))
- **codegen:** Anchor the ScanRef lifetime for a view with no borrowed field ([#250](https://github.com/hoodiecollin/forgedb/issues/250))
- **codegen:** Serde for arrays past the N = 32 ceiling ([#243](https://github.com/hoodiecollin/forgedb/issues/243))
- **lsp,inspector:** Teach the tools the named @length form ([#235](https://github.com/hoodiecollin/forgedb/issues/235))
- **codegen:** Key f64 indexes by an IEEE 754 total order ([#242](https://github.com/hoodiecollin/forgedb/issues/242))
- **codegen:** Compare @min/@max in the field's own numeric domain ([#239](https://github.com/hoodiecollin/forgedb/issues/239))
- **codegen:** Identify a staged &unique claim by its model, not field alone ([#257](https://github.com/hoodiecollin/forgedb/issues/257))
- **codegen:** Name the model in every ValidationError ([#257](https://github.com/hoodiecollin/forgedb/issues/257))

### Performance

- **codegen:** Filter the list and live-query scans on the borrowed view ([#224](https://github.com/hoodiecollin/forgedb/issues/224))
- **codegen:** Write nullable string/json tags without allocating ([#231](https://github.com/hoodiecollin/forgedb/issues/231))
- **codegen:** Emit index keys monomorphically per field type ([#230](https://github.com/hoodiecollin/forgedb/issues/230))
- **codegen:** Serialize REST reads from their types, not a Value tree ([#229](https://github.com/hoodiecollin/forgedb/issues/229))
- **storage-native:** Gather a sparse selection per row, not by span ([#228](https://github.com/hoodiecollin/forgedb/issues/228))
- **codegen:** Make the narrow scan a scope, and never materialize a row ([#228](https://github.com/hoodiecollin/forgedb/issues/228))
## [0.3.1] - 2026-08-01

### Performance

- **storage:** Map the spanned region in FixedColumn::gather ([#221](https://github.com/hoodiecollin/forgedb/issues/221))
- **storage:** Bound and map the spanned region in VariableColumn::gather_buffered ([#222](https://github.com/hoodiecollin/forgedb/issues/222))
## [0.3.0] - 2026-07-31

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

### Bug Fixes

- **init:** Refuse to start unauthenticated when FORGEDB_JWKS_URL is set ([#195](https://github.com/hoodiecollin/forgedb/issues/195))
- **parser:** Add missing Decimal arm to FieldType::size_in_bytes ([#189](https://github.com/hoodiecollin/forgedb/issues/189))
## [0.2.0] - 2026-07-28

> **Breaking release — upgrading needs action.** Required migration steps are in
> [docs/UPGRADING.md](https://github.com/hoodiecollin/forgedb/blob/main/docs/UPGRADING.md).

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

