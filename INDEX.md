# SinkDB - Documentation Index

## Overview

This directory contains documentation for **SinkDB**, a schema-first database system that transpiles declarative schemas into highly optimized columnar storage with code generation.

**Version**: 0.1.0 (Sprint 5 Complete)
**Last Updated**: October 13, 2025
**Status**: Core Implementation Complete

---

## Quick Navigation

### 🚀 Getting Started
- **[README.md](./README.md)** - Project overview, philosophy, and introduction
- **[EXAMPLES.md](./EXAMPLES.md)** - Complete examples and tutorials to get started

### 📋 Project Documentation
- **[SPRINT_PLAN.md](./SPRINT_PLAN.md)** - Overall sprint structure and development plan
- **[ROADMAP.md](./ROADMAP.md)** - Development phases and timeline

### 📚 Archived Documentation
Documentation from the design and implementation phases has been moved to the `archive/` directory:
- **[archive/planning/](./archive/planning/)** - Original design specifications (DSL, Storage, API, CLI)
- **[archive/sprint-summaries/](./archive/sprint-summaries/)** - Completed sprint implementation summaries

---

## Completed Sprints

### Implementation Status (Sprint 1-5 ✅)

Core features are now complete and tested:

- **Sprint 1** - MVP (Schema parsing, code generation, basic storage) ✅
- **Sprint 2** - Types & Persistence (9 type system, columnar storage, validation) ✅
- **Sprint 3** - Indexing & Queries (Hash indexes, query operations, tombstones) ✅
- **Sprint 4** - Relations (One-to-many, foreign key validation) ✅
- **Sprint 5** - Developer Experience (CLI, scaffolding, file watching, constraints, advanced indexes) ✅

**Detailed sprint documentation:** See `archive/sprint-summaries/` for implementation notes and test coverage.

---

## Document Descriptions

### README.md
**What it covers:**
- Executive summary
- Core innovations (compile-time optimization, columnar storage)
- Key features overview
- Target use cases
- Philosophy and design principles
- Related projects and differentiators

**Read this if:** You want a high-level understanding of SinkDB and its current implementation status.

---

### EXAMPLES.md
**What it covers:**
- Working code examples for all features
- Sprint-specific demonstrations
- Usage patterns and best practices

**Read this if:** You want to see how to use SinkDB features in practice.

---

### SPRINT_PLAN.md
**What it covers:**
- Sprint breakdown and progression
- Feature implementation order
- Sprint dependencies and milestones

**Read this if:** You want to understand the development approach and remaining work.

---

### ROADMAP.md
**What it covers:**
- Remaining sprints (6-20)
- Future features and capabilities
- Production readiness checklist

**Read this if:** You want to see what's coming next.

---

## Archived Documentation

### archive/planning/DSL_SPECIFICATION.md
**What it covers:**
- Complete schema language syntax
- Type system (primitives, strings, UUIDs, timestamps, financial, hashes)
- Symbols and operators (`+`, `~`, `^`, `&`, `?`, `*`, `$`, `#`)
- Directives (`@public`, `@computed`, `@indexed`, etc.)
- Relations (1:1, 1:many, many:many)
- Inline structs and fixed-size arrays
- Computed fields
- UI component integration
- Complete examples

**Read this if:** You need to write schemas or understand the language features.

---

### STORAGE_ARCHITECTURE.md
**What it covers:**
- Columnar storage layout
- Fixed-size vs variable-length data
- Memory-mapped files
- Tombstones and deletion
- Write-Ahead Log (WAL)
- Compaction strategy
- Query execution (columnar scanning, vectorization)
- Indexing (hash, B-tree, full-text)
- Concurrency model
- Performance characteristics

**Read this if:** You want to understand how data is stored and queried under the hood.

---

### API_GENERATION.md
**What it covers:**
- URL structure and patterns
- CRUD operations (List, Get, Create, Update, Delete)
- Query parameters (filtering, sorting, pagination)
- Relationship routes
- Batch operations
- Computed field RPC
- Error handling
- OpenAPI specification generation
- Authentication/authorization hooks
- Rate limiting and CORS
- Performance considerations

**Read this if:** You need to understand or use the auto-generated REST API.

---

### CLI_SPECIFICATION.md
**What it covers:**
- All CLI commands (`init`, `dev`, `generate`, `validate`, `migrate`, `build`, etc.)
- Project structure
- Configuration file (`sinkdb.toml`)
- Development workflow
- Watch mode and hot reload
- Migration management
- Testing and benchmarking
- Deployment
- Troubleshooting

**Read this if:** You'll be using the SinkDB CLI for development.

---

### ROADMAP.md
**What it covers:**
- Development phases (v1.0 - v3.0)
- Detailed milestones with timelines
- Testing strategy
- Benchmarking plans
- Risk management
- Success metrics
- Resource requirements
- Open questions

**Read this if:** You want to understand the implementation plan or contribute to development.

---

### archive/planning/ADVANCED_FEATURES.md
**What it covers:**
- Partial column selection (projection optimization)
- Hot cache / materialized in-memory views
- Tuple types for fixed multi-relationships
- Blockchain-based distributed transaction ledger
- WASM browser sync with live push updates
- Integration of all advanced features
- Implementation roadmap for experimental features

**Read this if:** You want to understand cutting-edge features and future directions.

---

## Key Concepts

### The Big Ideas

1. **Single Source of Truth**: One schema file defines database, API, types, and UI contracts
2. **Compile-Time Optimization**: Schema transpiles to specialized Rust code
3. **Columnar Storage**: Fixed-size data memory-mapped, variable-length append-only
4. **Type Safety**: End-to-end type safety from database to frontend
5. **Convention over Configuration**: Sensible defaults, explicit when needed

### The Schema Language

```
User {
  id: +uuid                    // Auto-generate UUID
  email: ^&string @email       // Indexed, unique, validated
  age: u32                     // Required by default
  bio: string?                 // ? = nullable
  
  created_at: +timestamp       // + = auto-set on create
  updated_at: ~timestamp       // ~ = auto-update on write
  
  posts: [Post]                // One-to-many relation
  
  full_name: string @computed  // Computed field
  
  profile: jsx://views/Profile.jsx  // UI component
}
```

### The Workflow

1. **Define schema** in `schema.lang`
2. **Run `sinkdb dev`** - generates code, starts server
3. **Implement stubs** - computed fields, UI components
4. **Build your app** - use type-safe APIs
5. **Deploy** - production-optimized binary

### The Generated Output

From one schema file, SinkDB generates:
- Rust database implementation (columnar storage)
- TypeScript types
- REST API server
- OpenAPI specification
- Component stubs
- Migration system

---

## Current Status (Sprint 5 Complete)

### ✅ Implemented Features

**Core Database:**
- Schema parsing and validation ✅
- Code generation (Rust storage layer) ✅
- Columnar storage (fixed + variable) ✅
- 9-type type system (u32, u64, i32, i64, f64, bool, string, uuid, timestamp) ✅
- Auto-generation (+uuid, +timestamp) ✅
- Unique constraints (&) ✅
- Hash indexes (^) ✅
- Composite indexes ✅
- One-to-many relations ✅
- Tombstone deletion ✅
- Schema constraints (@min, @max, @email, @url, @pattern) ✅

**Developer Tools:**
- CLI (`sinkdb init`, `generate`, `validate`, `build`) ✅
- Project scaffolding ✅
- File watching and auto-regeneration ✅
- Validation with helpful error messages ✅

**Testing:**
- 189+ comprehensive tests ✅
- Examples for all features ✅

### 🚧 Remaining Work (Sprints 6-20)

See SPRINT_PLAN.md and ROADMAP.md for:
- Write-Ahead Log (WAL) and crash recovery
- Update/delete operations
- API generation (REST)
- Query engine improvements
- Computed fields
- Many-to-many relations
- Transactions
- Optimization and production hardening

---

## Architecture Diagrams

### High-Level Flow

```
schema.lang
    ↓ (parse)
Abstract Syntax Tree (AST)
    ↓ (validate)
Semantic Model
    ↓ (generate)
    ├─→ Rust Code (db.rs)
    │   ├─ Storage layer
    │   ├─ Query engine
    │   └─ API server
    ├─→ TypeScript (types.ts)
    └─→ OpenAPI (openapi.yaml)
```

### Storage Layout

```
Database Directory
├── manifest.json          # Schema metadata
├── tombstones.bin        # Deletion bitmap
├── wal/                  # Write-ahead log
├── fixed/                # Fixed-size columns
│   ├── u64.bin          # All u64 fields
│   ├── u32.bin
│   ├── uuid.bin
│   └── structs.bin      # Inline structs
└── variable/            # Variable-length
    ├── string_data.bin  # String bytes
    └── string_offsets.bin
```

### Request Flow

```
HTTP Request
    ↓
Generated API Handler
    ↓
Query Parser (parse filters, sort, etc.)
    ↓
Query Executor
    ├─→ Columnar Scan
    ├─→ Index Lookup
    └─→ Join Operation
    ↓
Result Materialization
    ↓
Response Serialization
    ↓
HTTP Response (JSON)
```

---

## Example Schema (Blog)

```
User {
  id: +uuid
  email: ^&string @email
  username: ^&char(30)
  password_hash: #argon2(32) @private
  
  posts: [Post]
  comments: [Comment]
  
  created_at: +timestamp
  
  profile: jsx://views/UserProfile.jsx
}

Post {
  @versioned
  
  id: +uuid
  slug: ^&char(100)
  title: ^string
  content: string @fulltext
  
  author: *User
  category: *Category
  tags: [Tag]
  
  status: string {
    enum: ["draft", "published", "archived"]
    default: "draft"
  }
  
  view_count: +u64
  read_time: u32 @computed
  
  published_at: timestamp?
  created_at: +timestamp
  updated_at: ~timestamp
  
  detail: jsx://views/PostDetail.jsx
  preview: jsx://components/PostPreview.jsx
}

Category {
  id: +uuid
  name: &string
  slug: &char(50)
  posts: [Post]
}

Tag {
  id: +uuid
  name: &char(30)
  posts: [Post]
}

Comment {
  @soft_delete
  
  id: +uuid
  content: string
  post: *Post
  author: *User
  parent: Comment?
  
  created_at: +timestamp
}
```

---

## Performance Targets

### Storage
- Insert 1M rows: < 5s
- Sequential scan 1M rows: < 100ms
- Index lookup: < 1μs
- Memory usage: O(rows), not O(rows × columns)

### API
- REST endpoint p99: < 10ms
- Throughput: > 10k req/s (single core)

### Development
- Schema parse + codegen: < 1s for 100-model schema
- Hot reload: < 1s

---

## Technology Stack

### Core
- **Language**: Rust (database engine, API server)
- **Storage**: Memory-mapped files, columnar layout
- **Serialization**: Custom binary format
- **HTTP**: Axum or Actix-web (TBD)

### Code Generation
- **Parser**: Rust (nom or pest)
- **Templates**: Handlebars or Tera
- **Output**: Rust + TypeScript

### CLI
- **Framework**: clap
- **File watching**: notify
- **Process management**: tokio

---

## Comparisons

### vs SQLite
- **SinkDB**: Columnar, optimized for analytics + OLTP hybrid
- **SQLite**: Row-based, pure OLTP
- **Trade-off**: SinkDB requires schema at compile-time

### vs PostgreSQL
- **SinkDB**: Embedded, single-node, schema-first
- **PostgreSQL**: Client-server, distributed-ready, schema-flexible
- **Trade-off**: SinkDB sacrifices flexibility for performance

### vs Prisma (ORM)
- **SinkDB**: Database + ORM + API in one
- **Prisma**: ORM layer over existing databases
- **Trade-off**: SinkDB is more opinionated but more integrated

### vs PostgREST
- **SinkDB**: Generates optimized database + API
- **PostgREST**: Generates API over PostgreSQL
- **Trade-off**: SinkDB owns the storage layer

---

## Getting Help

### During Design Phase
- Review documentation in this directory
- Open issues/questions (GitHub - future)
- Join discussions (Discord - future)

### Once Released
- CLI: `sinkdb --help`
- Dev server docs: `http://localhost:3000/docs`
- API docs: OpenAPI spec at `/api/docs`

---

## Contributing (Future)

Once open-sourced:
1. Read `CONTRIBUTING.md`
2. Check `ROADMAP.md` for current priorities
3. Start with "good first issue" label
4. Submit PRs with tests

---

## License

TBD (likely MIT or Apache 2.0)

---

## Project Status

**Current Phase**: Design & Specification  
**Next Steps**: 
1. Finalize design decisions
2. Begin Phase 1 implementation
3. Build proof-of-concept
4. Performance validation

---

## Additional Resources

### Internal Documentation
- Architecture decision records (ADRs) - TBD
- Performance benchmarks - TBD
- Security considerations - TBD

### External Resources
- Project website - TBD
- Blog posts - TBD
- Video tutorials - TBD
- Community forum - TBD

---

## Quick Reference

### Common Commands
```bash
sinkdb init my-app          # Create new project
sinkdb dev                  # Start dev server
sinkdb generate             # Generate code
sinkdb validate             # Check schema
sinkdb migrate up           # Run migrations
sinkdb build --release      # Production build
```

### Common Patterns
```
// Auto-increment ID
id: +u64

// Auto-generate UUID
id: +uuid

// Indexed unique field
email: ^&string

// Nullable field
bio: string?

// Computed field
full_name: string @computed

// One-to-many relation
posts: [Post]

// Required relation
user: *User

// Inline struct
address: Address

// Fixed-size array
dimensions: [f64; 3]
```

---

## Changelog

### v0.1.0 - Sprint 5 (2025-10-13)
- ✅ Complete core database implementation (Sprints 1-5)
- ✅ CLI and developer tools
- ✅ Comprehensive test suite (189+ tests)
- ✅ Working examples for all features
- 📚 Documentation reorganization (archived design docs)

### v0.0.1 (2025-10-11)
- Initial design documentation
- Complete specifications
- Development roadmap

---

**Project Status**: Core implementation complete, ready for remaining sprints.

**Last Updated**: October 13, 2025
