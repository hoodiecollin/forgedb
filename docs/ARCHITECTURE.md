# ForgeDB Architecture

**Last Updated:** October 2025  
**Version:** 0.1.0

## Table of Contents

- [System Overview](#system-overview)
- [Component Architecture](#component-architecture)
- [Data Flow](#data-flow)
- [Public vs Internal Separation](#public-vs-internal-separation)
- [Design Decisions](#design-decisions)
- [Performance Characteristics](#performance-characteristics)

---

## System Overview

ForgeDB is a revolutionary database system that uses a declarative schema language to generate a complete full-stack application: database, type-safe APIs, UI components, and developer tooling. The system transpiles schema definitions into highly optimized Rust code with columnar storage, providing exceptional performance through compile-time optimization while maintaining perfect type safety across the entire stack.

### Core Innovation

**Single Source of Truth**: One schema file defines your entire application:
- Database storage layout (columnar, optimized)
- Type-safe Rust database implementation
- TypeScript types and client SDK
- REST API with OpenAPI specification
- UI component contracts
- Computed field contracts
- Migration system

### High-Level Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      Schema Definition                        │
│                      (schema.forge)                          │
└──────────────────────┬───────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                   ForgeDB Tooling                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │   Parser     │→ │  Validator   │→ │  Generator   │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
└──────────────────────┬───────────────────────────────────────┘
                       │
           ┌───────────┴───────────┐
           ▼                       ▼
┌─────────────────────┐  ┌─────────────────────┐
│   Rust Backend      │  │  TypeScript SDK     │
│  ┌───────────────┐  │  │  ┌───────────────┐  │
│  │    Storage    │  │  │  │     Types     │  │
│  │    Engine     │  │  │  │               │  │
│  ├───────────────┤  │  │  ├───────────────┤  │
│  │   CRUD API    │  │  │  │  API Client   │  │
│  ├───────────────┤  │  │  ├───────────────┤  │
│  │  HTTP Server  │  │  │  │  Components   │  │
│  └───────────────┘  │  │  └───────────────┘  │
└─────────────────────┘  └─────────────────────┘
```

---

## Component Architecture

ForgeDB consists of 15 crates organized into two categories: **Public (Runtime)** and **Internal (Tooling)**.

### Public Crates (Runtime Libraries)

These crates provide functionality that generated code uses at runtime. They are designed to be published to crates.io and used independently.

#### Core Storage Layer

##### forgedb-types
**Purpose**: Core type definitions and traits used across the system.

**Key Components**:
- Type system traits
- Common data structures
- Shared error types

##### forgedb-storage
**Purpose**: Columnar storage engine with memory-mapped file access.

**Key Components**:
- `Database`: Main storage entry point
- `FixedColumn`: Fixed-size column storage (u64, i64, f64, uuid)
- `VariableColumn`: Variable-length data storage (strings)
- `Tombstones`: Deletion tracking bitmap
- `Manifest`: Database metadata

**Architecture**:
```
data/
├── manifest.json              # Schema metadata
├── tombstones.bin            # Deletion bitmap
├── fixed/                    # Fixed-size columns
│   └── u64_0.bin
└── variable/                 # Variable-length columns
    ├── string_data_0.bin
    └── string_offsets_0.bin
```

##### forgedb-wal
**Purpose**: Write-Ahead Log for ACID properties and crash recovery.

**Key Components**:
- `WalManager`: High-level WAL interface
- `Transaction`: Atomic operation grouping
- `WalEntry`: Individual log entries
- `FsyncPolicy`: Durability control

#### API & Query Layer

##### forgedb-crud-api
**Purpose**: High-level CRUD operations on top of storage primitives.

**Key Components**:
- Type-safe insert/update/delete operations
- Query execution
- Relationship traversal

##### forgedb-query-params
**Purpose**: HTTP query parameter parsing and validation.

**Key Components**:
- Query string parsing
- Filter expression evaluation
- Pagination parameters

##### forgedb-query-optimization
**Purpose**: SIMD-optimized query execution over columnar data.

**Key Components**:
- Query plan optimization
- SIMD operations on fixed-size columns
- Index utilization

##### forgedb-fulltext
**Purpose**: Full-text search capabilities.

**Key Components**:
- Text indexing
- Search query parsing
- Ranking algorithms

#### Server & Infrastructure

##### forgedb-http-server
**Purpose**: Production-ready HTTP server infrastructure.

**Key Features**:
- Axum-based async HTTP handling
- TLS/HTTPS support
- Authentication (JWT, API keys)
- Rate limiting
- Response caching
- Health checks
- Prometheus metrics

##### forgedb-compaction
**Purpose**: Background compaction for reclaiming tombstone space.

**Key Components**:
- Compaction scheduler
- Space reclamation
- Row remapping

##### forgedb-ffi
**Purpose**: Foreign Function Interface for C/C++ integration.

**Key Components**:
- C-compatible API bindings
- Memory safety wrappers

### Internal Crates (Tooling)

These crates handle code generation, CLI, and development tools. They remain in the main repository.

##### forgedb-parser
**Purpose**: Schema language parser and AST generator.

**Key Components**:
- Lexer/tokenizer
- Parser combinators
- AST data structures
- Error reporting

**Supported Features**:
- Model definitions
- Field types and modifiers
- Relationships (one-to-many, many-to-many)
- Directives (@index, @pattern, @min, @max)
- Component fields (tsx://, jsx://, api://)

##### forgedb-validation
**Purpose**: Semantic validation of schema AST.

**Key Components**:
- Type checking
- Relationship validation
- Directive validation
- Constraint verification

##### forgedb-watcher
**Purpose**: File system watching for hot reload during development.

**Key Components**:
- File change detection
- Debouncing
- Event notification

##### forgedb-migrations
**Purpose**: Schema migration generation and execution.

**Key Components**:
- Migration plan generation
- DDL operation execution
- Schema versioning
- Rollback support

##### forgedb-lsp-server
**Purpose**: Language Server Protocol implementation for IDE support.

**Key Features**:
- Syntax highlighting
- Auto-completion
- Go-to-definition
- Error diagnostics
- Hover documentation

##### forgedb CLI (binary)
**Purpose**: Command-line interface for ForgeDB operations.

**Commands**:
- `init`: Create new project
- `dev`: Start development server with hot reload
- `build`: Generate production code
- `migrate`: Run migrations
- `validate`: Validate schema

---

## Data Flow

### Development Flow

```
1. Developer writes schema.forge
   ↓
2. CLI watches for changes (forgedb-watcher)
   ↓
3. Schema parsed (forgedb-parser)
   ↓
4. Schema validated (forgedb-validation)
   ↓
5. Code generated (CLI generator)
   ↓
6. Generated code compiled
   ↓
7. Server restarted with new schema
```

### Request Flow

```
1. HTTP Request
   ↓
2. forgedb-http-server (routing, auth, rate limiting)
   ↓
3. forgedb-query-params (parse query parameters)
   ↓
4. forgedb-crud-api (business logic)
   ↓
5. forgedb-query-optimization (query planning)
   ↓
6. forgedb-storage (data access)
   ↓
7. forgedb-wal (transactional writes)
   ↓
8. Response with caching headers
```

### Write Flow (Transactional)

```
1. Insert/Update/Delete request
   ↓
2. Begin transaction (forgedb-wal)
   ↓
3. Write to WAL log
   ↓
4. Apply to storage (forgedb-storage)
   ↓
5. Commit transaction
   ↓
6. Fsync based on policy
   ↓
7. Return success
```

### Background Processes

```
┌──────────────────┐     ┌──────────────────┐
│  Log Compaction  │     │  Space Recovery  │
│  (Periodic)      │     │  (Threshold)     │
└────────┬─────────┘     └────────┬─────────┘
         │                        │
         ▼                        ▼
    ┌────────────────────────────────┐
    │      forgedb-compaction        │
    │  - Tombstone cleanup           │
    │  - Row remapping               │
    │  - File consolidation          │
    └────────────────────────────────┘
```

---

## Public vs Internal Separation

### Public Crates Design Principles

Public crates are designed to be:

1. **Standalone**: Can be used independently without ForgeDB tooling
2. **Stable**: Semantic versioning guarantees
3. **Well-documented**: Comprehensive docs and examples
4. **Zero tooling dependencies**: No dependencies on internal crates
5. **Production-ready**: Battle-tested with comprehensive tests

**Public Crate Dependencies**:
```
forgedb-types (foundation)
  ↓
forgedb-storage ← forgedb-wal
  ↓
forgedb-crud-api
  ↓
forgedb-http-server → forgedb-query-params
                    → forgedb-query-optimization
                    → forgedb-fulltext
                    → forgedb-compaction
```

### Internal Crates Design Principles

Internal crates are designed for:

1. **Code generation**: Transform schema to runnable code
2. **Developer experience**: CLI, hot reload, LSP
3. **Rapid iteration**: Breaking changes acceptable
4. **Tooling focus**: Not used in production runtime

**Internal Crate Dependencies**:
```
forgedb-parser
  ↓
forgedb-validation
  ↓
CLI Generator → forgedb-watcher
              → forgedb-migrations
              → forgedb-lsp-server
```

### Dependency Rules

**✅ Allowed**:
- Public crates can depend on other public crates
- Internal crates can depend on other internal crates
- Internal crates can depend on public crates (for code generation)

**❌ Not Allowed**:
- Public crates MUST NOT depend on internal crates
- Generated code MUST NOT depend on internal crates

### Future Repository Split

The crates are organized to eventually split into:

**forgedb-runtime** (public repository):
- All public crates
- Published to crates.io
- Stable API guarantees
- Community contributions welcome

**forgedb** (main repository):
- All internal crates
- CLI binary
- Code generators
- Development tools

---

## Design Decisions

### 1. Columnar Storage

**Decision**: Use columnar storage instead of row-based storage.

**Rationale**:
- **Query performance**: Only load columns needed for query
- **Compression**: Better compression ratios for homogeneous data
- **Cache efficiency**: Sequential access patterns improve CPU cache utilization
- **SIMD operations**: Vectorized operations on numeric columns
- **Memory efficiency**: Sparse column access reduces memory footprint

**Trade-offs**:
- Row reconstruction requires reading multiple columns
- Write patterns are column-wise (less natural for some operations)
- Schema changes require more work

### 2. Memory-Mapped Files

**Decision**: Use memory-mapped file access for storage.

**Rationale**:
- **Zero-copy reads**: Direct access to file contents
- **OS-managed caching**: Operating system handles page cache
- **Lazy loading**: Only accessed pages loaded into memory
- **Reduced memory footprint**: OS controls memory usage

**Trade-offs**:
- Platform-specific behavior
- Error handling complexity
- Address space limitations on 32-bit systems

### 3. Write-Ahead Log

**Decision**: Implement WAL with configurable fsync policy.

**Rationale**:
- **ACID guarantees**: Transaction atomicity and durability
- **Crash recovery**: Replay log after failures
- **Performance tuning**: Fsync policy trades durability for speed
- **Point-in-time recovery**: Can restore to any transaction

**Trade-offs**:
- Write amplification (data written twice)
- Storage overhead (log file size)
- Complexity in recovery logic

### 4. Tombstone Deletion

**Decision**: Use tombstone bitmap instead of physical deletion.

**Rationale**:
- **Fast deletes**: O(1) operation (flip bit)
- **No data movement**: Preserves row IDs
- **Simple implementation**: Single bitmap file
- **Deferred compaction**: Reclaim space in background

**Trade-offs**:
- Storage overhead for deleted rows
- Requires periodic compaction
- Queries must skip tombstones

### 5. Compile-Time Code Generation

**Decision**: Transpile schema to specialized Rust code.

**Rationale**:
- **Type safety**: Compile-time verification
- **Performance**: Monomorphization eliminates indirection
- **Optimization**: Rust compiler optimizes for specific schema
- **No runtime overhead**: No generic lookups or dynamic dispatch

**Trade-offs**:
- Schema changes require recompilation
- Larger binary size (specialized code)
- Less flexible than runtime-generic databases

### 6. Axum for HTTP Server

**Decision**: Use Axum web framework instead of alternatives.

**Rationale**:
- **Type safety**: Extract-transform-respond pattern
- **Performance**: Built on Tokio and Tower
- **Ecosystem**: Excellent middleware ecosystem
- **Developer experience**: Ergonomic API design
- **Async**: Native async/await support

**Alternatives considered**:
- Actix-web (more complex API)
- Warp (less maintained)
- Rocket (not async-native at time of decision)

### 7. Modular Crate Architecture

**Decision**: Split functionality into 15 focused crates.

**Rationale**:
- **Separation of concerns**: Clear boundaries between components
- **Reusability**: Crates usable independently
- **Parallel development**: Teams can work on different crates
- **Smaller compile times**: Only rebuild changed crates
- **Public/private split**: Easy to extract runtime crates later

**Trade-offs**:
- More complex dependency management
- Increased maintenance overhead
- Potential version coordination issues

### 8. LSP for Editor Support

**Decision**: Implement Language Server Protocol for ForgeDB schema language.

**Rationale**:
- **Editor-agnostic**: Works with any LSP-compatible editor
- **Rich features**: Auto-completion, diagnostics, hover, go-to-definition
- **Real-time validation**: Immediate feedback on schema errors
- **Developer experience**: Professional IDE experience

**Trade-offs**:
- Development complexity
- Maintenance burden
- Memory overhead for language server process

---

## Performance Characteristics

### Storage Layer

**Fixed-Size Columns**:
- Write: O(1) append
- Read: O(1) random access
- Sequential scan: ~5-10 GB/s (memory bandwidth limited)
- Overhead: 0 bytes per value

**Variable-Length Columns**:
- Write: O(1) append (data + offset)
- Read: O(1) random access (two seeks)
- Sequential scan: ~2-4 GB/s
- Overhead: 16 bytes per value (offset + length)

**Tombstones**:
- Write: O(1) single byte write
- Read: O(1) single byte read
- Overhead: 1 byte per row

### Query Performance

**Index Lookups**:
- Unique index: O(1) hash lookup
- Range query: O(log n + k) where k is result size
- Full table scan: O(n) with SIMD acceleration

**Joins**:
- Hash join: O(n + m)
- Nested loop: O(n * m) (avoided when possible)

**Aggregations**:
- COUNT/SUM with SIMD: 10-20x faster than scalar
- MIN/MAX on indexed column: O(1)

### HTTP Server

**Request Latency** (p99):
- Simple query: < 5ms
- Complex query with joins: < 50ms
- Full table scan (100K rows): < 100ms

**Throughput**:
- Read operations: > 10K req/s (single instance)
- Write operations: > 5K req/s (limited by fsync)
- With response caching: > 50K req/s

### Memory Usage

**Base Overhead**:
- Empty database: ~1 MB (manifest + structures)
- Per 1M rows: ~1-2 MB (tombstones + metadata)

**Active Memory**:
- Controlled by OS page cache
- Only accessed pages loaded
- Typical: 10-20% of database size

### Disk I/O

**Sequential Reads**:
- Columnar scan: 500-1000 MB/s (SSD)
- Limited by disk bandwidth

**Random Reads**:
- Index lookup: < 1ms per lookup (SSD)
- Cache hit: < 1μs

**Writes**:
- With WAL + fsync: ~1K writes/s (fsync limited)
- With WAL batch: ~50K writes/s
- Without durability: > 100K writes/s

### Compilation

**Code Generation**:
- Schema parsing: < 10ms
- Code generation: < 100ms
- Rust compilation: 30-60s (first build), 1-5s (incremental)

**Binary Size**:
- Base runtime: ~10 MB
- Per model: ~100-500 KB (depends on fields/indexes)

---

## Security Considerations

### Authentication

- JWT token validation
- API key authentication
- Role-based access control (RBAC)
- Pluggable auth hooks

### Network Security

- TLS/HTTPS support (rustls)
- Rate limiting (token bucket)
- Request size limits
- CORS configuration

### Data Security

- No SQL injection (type-safe queries)
- Input validation
- Output sanitization
- Prepared statements in generated code

### Operational Security

- Secure secret management
- Log sanitization (no sensitive data)
- Health checks (no information leakage)
- Metrics (no PII exposure)

---

## Monitoring & Observability

### Metrics (Prometheus)

- Request latency histograms
- Error rates by type
- Active connections
- Cache hit/miss rates
- Database operation durations

### Health Checks

- Liveness probe: Server running
- Readiness probe: Can accept traffic
- Custom health checkers: Database connectivity

### Logging (Tracing)

- Structured JSON logs
- Trace IDs for request tracking
- Configurable log levels
- Error stack traces

---

## Future Architecture Evolution

### Phase 1: Current State
- Monolithic Rust backend
- Code generation from schema
- Basic HTTP API

### Phase 2: Planned
- WebAssembly compilation target
- Browser-based database
- Offline-first applications
- Local-first sync

### Phase 3: Future
- Distributed database cluster
- Horizontal scaling
- Replication protocols
- Multi-region deployment

### Phase 4: AI Integration
- AI agents implement components
- Natural language schema definition
- Automated optimization suggestions
- Intelligent query planning

---

## References

- [Public Crates Documentation](./PUBLIC_CRATES.md)
- [Internal Crates Documentation](./INTERNAL_CRATES.md)
- [Development Guide](./DEVELOPMENT.md)
- [Contributing Guide](./CONTRIBUTING.md)
- [Publishing Process](./PUBLISHING.md)

---

**Maintained by**: ForgeDB Team  
**Questions?**: Open an issue on GitHub
