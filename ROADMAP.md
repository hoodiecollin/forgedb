# TypeDB Development Roadmap

## Overview

This document outlines the development phases for TypeDB, from initial prototype through production-ready system and advanced features.

## Development Philosophy

- **Iterative**: Build working prototypes early, refine continuously
- **Test-driven**: Comprehensive tests at each phase
- **Performance-focused**: Benchmark early and often
- **Documentation-first**: Specify before implementing

## Phase 1: Core Database Engine (v0.1 - v1.0)

**Goal**: Working database with columnar storage, basic CRUD operations, and schema transpiler.

**Timeline**: 6-8 months

### Milestone 1.1: Schema Parser & AST (Weeks 1-3)

**Deliverables:**
- [ ] Schema language lexer
- [ ] Recursive descent parser
- [ ] Abstract Syntax Tree (AST) representation
- [ ] Semantic validation
- [ ] Error reporting with line numbers

**Files to Create:**
```
typedb/
├── schema-lang/
│   ├── lexer.rs         # Tokenization
│   ├── parser.rs        # Parsing logic
│   ├── ast.rs           # AST data structures
│   ├── validator.rs     # Semantic validation
│   └── error.rs         # Error types
```

**Tests:**
- Valid schema parsing
- Error cases (syntax errors, semantic errors)
- All type combinations
- Relationship declarations

**Success Criteria:**
- Parse all example schemas from DSL_SPECIFICATION.md
- Helpful error messages
- < 100ms parse time for 1000-line schema

---

### Milestone 1.2: Code Generation Framework (Weeks 4-6)

**Deliverables:**
- [ ] Code generator architecture
- [ ] Rust code generation
- [ ] TypeScript type generation
- [ ] Template system for generated code

**Files to Create:**
```
typedb/
├── transpiler/
│   ├── codegen/
│   │   ├── mod.rs
│   │   ├── rust.rs      # Rust code generation
│   │   ├── typescript.rs # TS type generation
│   │   └── templates/   # Code templates
│   ├── ir.rs            # Intermediate representation
│   └── manifest.rs      # Schema manifest
```

**Tests:**
- Generate valid Rust code from AST
- Generated code compiles
- TypeScript types are valid
- Round-trip: schema -> code -> types

**Success Criteria:**
- Generated Rust code passes clippy
- TypeScript types pass tsc --noEmit
- Readable, idiomatic generated code

---

### Milestone 1.3: Basic Storage Engine (Weeks 7-10)

**Deliverables:**
- [ ] Memory-mapped file handling
- [ ] Fixed-size column storage (u64, u32, etc.)
- [ ] Tombstone bitmap
- [ ] Manifest management
- [ ] Insert, delete operations

**Files to Create:**
```
typedb/
├── runtime/
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── mmap.rs      # Memory-mapped files
│   │   ├── fixed.rs     # Fixed-size columns
│   │   ├── manifest.rs  # Chunk directory
│   │   └── tombstone.rs # Delete tracking
│   └── types.rs         # Runtime type system
```

**Tests:**
- Insert 1M rows
- Memory map correctness
- Tombstone marking
- Crash and reload (persistence)

**Success Criteria:**
- Insert 100k u64 rows < 100ms
- Memory usage = O(row_count)
- Survive process restart

---

### Milestone 1.4: Variable-Length Storage (Weeks 11-13)

**Deliverables:**
- [ ] String storage (append-only + offsets)
- [ ] Variable-length column implementation
- [ ] Update operations
- [ ] Basic compaction

**Files to Create:**
```
typedb/
├── runtime/
│   ├── storage/
│   │   ├── variable.rs   # Variable-length storage
│   │   ├── compaction.rs # Compaction logic
│   │   └── wal.rs        # Write-ahead log (basic)
```

**Tests:**
- Store 100k strings
- Update strings (longer and shorter)
- Compaction reduces file size
- No data loss during compaction

**Success Criteria:**
- Insert 100k strings < 500ms
- Compaction removes 90%+ dead space
- String access < 1μs (hot cache)

---

### Milestone 1.5: Query Execution (Weeks 14-17)

**Deliverables:**
- [ ] Columnar scanning
- [ ] Predicate evaluation
- [ ] Basic indexing (hash maps)
- [ ] Query API

**Files to Create:**
```
typedb/
├── runtime/
│   ├── query/
│   │   ├── mod.rs
│   │   ├── scan.rs       # Column scanning
│   │   ├── filter.rs     # Predicate evaluation
│   │   ├── index.rs      # Hash & B-Tree indices
│   │   └── executor.rs   # Query execution
```

**Tests:**
- Filter on single column
- Multi-column filters (AND/OR)
- Index lookups
- Query correctness vs naive implementation

**Success Criteria:**
- Scan 1M rows, filter on u32 < 100ms
- Index lookup < 1μs
- Correct results for complex queries

---

### Milestone 1.6: Relations & Joins (Weeks 18-20)

**Deliverables:**
- [ ] Foreign key columns
- [ ] Junction tables (many-to-many)
- [ ] Join implementation
- [ ] Relationship traversal API

**Files to Create:**
```
typedb/
├── runtime/
│   ├── query/
│   │   ├── join.rs       # Join algorithms
│   │   └── relation.rs   # Relation traversal
```

**Tests:**
- 1:1 relations
- 1:many relations
- many:many relations
- Nested joins

**Success Criteria:**
- Join 10k users to 100k posts < 50ms
- Correct cascade delete behavior
- Junction table generation works

---

### Milestone 1.7: Write-Ahead Log & Durability (Weeks 21-23)

**Deliverables:**
- [ ] Complete WAL implementation
- [ ] Transaction boundaries
- [ ] Crash recovery
- [ ] Fsync policies

**Files to Create:**
```
typedb/
├── runtime/
│   ├── wal/
│   │   ├── mod.rs
│   │   ├── writer.rs     # WAL writing
│   │   ├── reader.rs     # WAL reading
│   │   └── recovery.rs   # Crash recovery
```

**Tests:**
- Insert + crash + recover
- Transaction atomicity
- Recovery from partial writes
- WAL rotation

**Success Criteria:**
- No data loss on crash
- Recovery time < 1s for 10k writes
- Transaction ACID properties

---

### Milestone 1.8: Inline Structs & Arrays (Weeks 24-26)

**Deliverables:**
- [ ] Struct storage layout
- [ ] Fixed-size array support
- [ ] Nested struct handling
- [ ] Zero-copy struct access

**Files to Create:**
```
typedb/
├── runtime/
│   ├── storage/
│   │   └── structs.rs    # Inline struct storage
```

**Tests:**
- Store structs with various fields
- Nested structs
- Fixed-size arrays in structs
- Access struct fields without deserialization

**Success Criteria:**
- Struct access is zero-copy
- Correct alignment and padding
- SIMD-friendly layout

---

### Milestone 1.9: Integration & Polish (Weeks 27-30)

**Deliverables:**
- [ ] End-to-end transpiler + runtime
- [ ] Example applications
- [ ] Performance benchmarks
- [ ] Documentation
- [ ] Bug fixes

**Tasks:**
- Integration tests (schema -> code -> database)
- Build example blog, e-commerce apps
- Benchmark suite
- User documentation
- API documentation

**Success Criteria:**
- Can build working app from schema alone
- Benchmarks meet targets (see STORAGE_ARCHITECTURE.md)
- Documentation covers all features

---

## Phase 2: Full-Stack Integration (v1.1 - v2.0)

**Goal**: Developer tooling, API generation, UI integration, hot reload.

**Timeline**: 4-6 months after v1.0

### Milestone 2.1: CLI Tool (Weeks 31-34)

**Deliverables:**
- [ ] `typedb` CLI binary
- [ ] Project initialization
- [ ] Schema validation
- [ ] Code generation commands
- [ ] Dev server

**Commands:**
```bash
typedb init <project>
typedb dev
typedb generate
typedb validate
typedb build
```

**Files to Create:**
```
typedb/
├── cli/
│   ├── main.rs
│   ├── commands/
│   │   ├── init.rs
│   │   ├── dev.rs
│   │   ├── generate.rs
│   │   └── validate.rs
│   ├── project.rs       # Project structure
│   └── watcher.rs       # File watching
```

**Tests:**
- Project creation
- Schema watching
- Code regeneration
- Dev server startup

**Success Criteria:**
- < 100ms cold start
- Hot reload < 1s
- Clear error messages

---

### Milestone 2.2: REST API Generation (Weeks 35-38)

**Deliverables:**
- [ ] HTTP server framework
- [ ] CRUD endpoint generation
- [ ] Query parameter parsing
- [ ] OpenAPI spec generation
- [ ] Request validation

**Generated Routes:**
```
GET    /api/users
GET    /api/users/{id}
POST   /api/users
PUT    /api/users/{id}
PATCH  /api/users/{id}
DELETE /api/users/{id}
GET    /api/users/{id}/posts
```

**Files to Create:**
```
typedb/
├── runtime/
│   ├── api/
│   │   ├── mod.rs
│   │   ├── server.rs     # HTTP server
│   │   ├── routes.rs     # Route generation
│   │   ├── query.rs      # Query param parsing
│   │   └── openapi.rs    # OpenAPI spec gen
```

**Tests:**
- CRUD operations via HTTP
- Query parameters work
- Validation errors return 400
- OpenAPI spec validates

**Success Criteria:**
- < 5ms p99 latency for simple queries
- OpenAPI spec passes validation
- Type-safe query parameters

---

### Milestone 2.3: Computed Fields & Plugins (Weeks 39-42)

**Deliverables:**
- [ ] Computed field trait generation
- [ ] Client-side computation
- [ ] Server-side plugin system (WASM)
- [ ] Plugin loading
- [ ] RPC for computed fields

**Files to Create:**
```
typedb/
├── runtime/
│   ├── computed/
│   │   ├── mod.rs
│   │   ├── traits.rs     # Generated traits
│   │   ├── plugin.rs     # Plugin loading (WASM)
│   │   └── rpc.rs        # Computed field RPC
```

**Tests:**
- Client-side computed fields
- WASM plugin loading
- Computed field RPC
- Error handling

**Success Criteria:**
- Client computation: zero overhead
- WASM plugin load < 10ms
- Plugin isolation (sandboxing)

---

### Milestone 2.4: UI Component Integration (Weeks 43-46)

**Deliverables:**
- [ ] Component reference parsing
- [ ] Stub generation
- [ ] Props type generation
- [ ] SSR support
- [ ] Hot reload for components

**Files to Create:**
```
typedb/
├── cli/
│   ├── scaffold.rs       # Stub generation
│   └── ssr.rs           # Server-side rendering
├── runtime/
│   └── ui/
│       ├── mod.rs
│       ├── props.rs      # Props generation
│       └── renderer.rs   # SSR renderer
```

**Tests:**
- Stub creation
- Type-safe props
- SSR rendering
- Hot reload

**Success Criteria:**
- Component stub in < 100ms
- Props match schema
- SSR works with React/Svelte

---

### Milestone 2.5: Migrations (Weeks 47-50)

**Deliverables:**
- [ ] Schema diff algorithm
- [ ] Migration generation
- [ ] Migration execution
- [ ] Rollback support
- [ ] Data migration helpers

**Commands:**
```bash
typedb migrate create "add users"
typedb migrate up
typedb migrate down
typedb migrate status
```

**Files to Create:**
```
typedb/
├── cli/
│   └── migrate.rs
├── transpiler/
│   └── diff.rs          # Schema diffing
└── runtime/
    └── migrate/
        ├── mod.rs
        ├── executor.rs   # Migration execution
        └── rollback.rs   # Rollback logic
```

**Tests:**
- Add column migration
- Remove column migration
- Type change (breaking)
- Rollback works

**Success Criteria:**
- Safe migrations (no data loss)
- Reversible when possible
- Clear warnings for breaking changes

---

### Milestone 2.6: Polish & Documentation (Weeks 51-52)

**Deliverables:**
- [ ] Comprehensive docs
- [ ] Tutorial/quickstart
- [ ] Example projects
- [ ] Performance tuning guide
- [ ] Troubleshooting guide

**Documentation:**
- Getting started
- Schema language guide
- API reference
- CLI reference
- Best practices
- Performance optimization

**Success Criteria:**
- New user can build app in < 1 hour
- All features documented
- 5+ example projects

---

## Phase 3: Advanced Features (v2.1 - v3.0)

**Goal**: Production hardening, advanced features, AI integration.

**Timeline**: TBD (6-12 months after v2.0)

### Feature 3.1: WASM Support

**Deliverables:**
- [ ] Compile database to WASM
- [ ] Browser storage (IndexedDB/OPFS)
- [ ] Shared types across native + WASM
- [ ] Sync protocol

**Use Cases:**
- Local-first apps
- Offline-capable web apps
- Identical logic client + server

---

### Feature 3.2: Distributed / Replication

**Deliverables:**
- [ ] Multi-node support
- [ ] Leader election
- [ ] Replication protocol
- [ ] Consistency guarantees

**Not in scope for v1-v2:**
- Focus on single-node performance first
- Add distribution when core is solid

---

### Feature 3.3: Advanced Indexing

**Deliverables:**
- [ ] Full-text search (trigrams, BM25)
- [ ] Spatial indices (R-tree)
- [ ] Vector embeddings (HNSW)
- [ ] Composite indices

---

### Feature 3.4: MVCC & Advanced Concurrency

**Deliverables:**
- [ ] Multi-version concurrency control
- [ ] Snapshot isolation
- [ ] Optimistic concurrency
- [ ] Better write concurrency

---

### Feature 3.5: HTTP Remote Fields (v2 feature mentioned earlier)

**Deliverables:**
- [ ] HTTP field type
- [ ] Lazy fetching
- [ ] Response caching
- [ ] Type-safe remote schemas

---

### Feature 3.6: AI-Powered Development (v3)

**Deliverables:**
- [ ] `@ai-implement` annotation support
- [ ] Code generation via LLM
- [ ] Test generation
- [ ] Interactive AI assistant in CLI

**Example:**
```
User {
  /**
   * @ai-implement
   * Create a user profile component...
   */
  profile: jsx://views/Profile.jsx
}
```

```bash
$ typedb dev --ai
🤖 AI analyzing schema...
📝 Generating Profile.jsx...
✓ Created component with tests
```

---

## Testing Strategy

### Unit Tests
- Every module has comprehensive unit tests
- Property-based testing for storage/serialization
- Fuzzing for parser

### Integration Tests
- End-to-end: schema -> code -> database -> query
- Multi-model operations
- Transactions
- Recovery scenarios

### Performance Tests
- Microbenchmarks for storage operations
- Query benchmarks against known datasets
- Scalability tests (1M, 10M, 100M rows)
- Comparison benchmarks vs SQLite, PostgreSQL

### Acceptance Tests
- Build real applications
- User workflow tests
- Documentation examples work

---

## Benchmarking Plan

### Phase 1 Benchmarks

**Storage:**
- [ ] Insert 1M rows: target < 5s
- [ ] Update 100k rows: target < 2s
- [ ] Delete 100k rows (tombstone): target < 100ms
- [ ] Compaction 1M rows with 20% deleted: target < 3s

**Query:**
- [ ] Sequential scan 1M rows with numeric filter: target < 100ms
- [ ] Indexed lookup on 1M rows: target < 1μs
- [ ] Join 10k users to 100k posts: target < 50ms

**Memory:**
- [ ] Memory usage = O(rows) not O(rows * columns)
- [ ] 1M u64 column: target ~8MB (plus OS overhead)

### Phase 2 Benchmarks

**API:**
- [ ] REST endpoint p99: target < 10ms
- [ ] Throughput: target > 10k req/s (single core)

**Code Generation:**
- [ ] Schema parse + codegen: target < 1s for 100-model schema

### Comparison Benchmarks

Compare against:
- SQLite (single node OLTP)
- DuckDB (columnar analytics)
- PostgreSQL (reference RDBMS)

Metrics:
- Read latency (p50, p99)
- Write latency
- Throughput (reads/writes per second)
- Memory usage
- Disk space usage

---

## Risk Management

### Technical Risks

**Risk**: Memory-mapped files don't work well on all platforms
- **Mitigation**: Fallback to read/write APIs
- **Timeline impact**: +2 weeks if needed

**Risk**: Columnar storage not optimal for all workloads
- **Mitigation**: Benchmarks early (Phase 1.5)
- **Timeline impact**: Could require architecture pivot

**Risk**: Generated code is unreadable/unmaintainable
- **Mitigation**: Focus on clean codegen, use rustfmt
- **Timeline impact**: None if addressed early

**Risk**: Transpiler bugs cause incorrect code
- **Mitigation**: Extensive testing, fuzzing
- **Timeline impact**: +1-2 weeks for hardening

### Project Risks

**Risk**: Scope creep
- **Mitigation**: Strict phase boundaries, defer to v2/v3
- **Timeline impact**: Controllable

**Risk**: Performance doesn't meet targets
- **Mitigation**: Profile and optimize continuously
- **Timeline impact**: +2-4 weeks for optimization

**Risk**: Developer experience is confusing
- **Mitigation**: User testing, documentation focus
- **Timeline impact**: +1-2 weeks for UX polish

---

## Success Metrics

### Phase 1 (Core Engine)
- ✅ All storage operations meet performance targets
- ✅ Can build example blog/e-commerce app
- ✅ Zero data loss under crash scenarios
- ✅ 90%+ test coverage

### Phase 2 (Full-Stack)
- ✅ New user can build app in < 1 hour
- ✅ API endpoints match OpenAPI spec
- ✅ Hot reload works reliably
- ✅ 5+ example projects

### Phase 3 (Advanced)
- ✅ Production deployments with real users
- ✅ WASM version works in browsers
- ✅ AI code generation accuracy > 80%
- ✅ Community adoption and contributions

---

## Resource Requirements

### Phase 1: 1-2 full-time engineers
- Core database engineer (Rust expert)
- Optional: Compiler/codegen specialist

### Phase 2: 2-3 full-time engineers
- Add: Full-stack/API engineer
- Add: DevEx/tooling engineer

### Phase 3: 3-4 full-time engineers
- Add: Distributed systems engineer (if needed)
- Add: ML/AI integration engineer (for v3)

---

## Open Questions

- [ ] Choose HTTP framework for API server (axum, actix-web, warp?)
- [ ] WAL file format (custom vs existing like SQLite WAL?)
- [ ] Compaction strategy: background thread vs explicit trigger?
- [ ] TypeScript codegen: ESM vs CommonJS vs both?
- [ ] Testing framework: built-in vs external test suite?
- [ ] License: MIT, Apache 2.0, or dual-license?

---

**Document Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Planning Phase
