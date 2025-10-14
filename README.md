# SinkDB - Type-Safe, Schema-First Full-Stack Database Framework

## Executive Summary

SinkDB is a revolutionary database system that uses a declarative schema language to generate a complete full-stack application: database, type-safe APIs, UI components, and developer tooling. The system transpiles schema definitions into highly optimized Rust code with columnar storage, providing exceptional performance through compile-time optimization while maintaining perfect type safety across the entire stack.

## Core Innovation

**Single Source of Truth**: One schema file defines your entire application:
- Database storage layout (columnar, optimized)
- Type-safe Rust database implementation
- TypeScript types and client SDK
- REST API with OpenAPI specification
- UI component contracts
- Computed field contracts
- Migration system

## Key Features

### Compile-Time Optimization
- Schema transpiles to specialized Rust code
- No runtime overhead from generic data structures
- Monomorphization eliminates indirection
- SIMD-friendly columnar layout for numeric operations

### Columnar Hybrid Storage
- Fixed-size types (u64, f64, uuid, etc.): Memory-mapped pages for zero-copy access
- Variable-length data (strings): Append-only with offset indices
- Inline structs: Deterministic fixed-size compound data
- Optimal cache locality and vectorization

### Type Safety End-to-End
- Schema → Rust types → TypeScript types
- Impossible to have schema drift
- Compile-time verification
- Generated API contracts

### Auto-Generated REST API
- CRUD operations for all models
- Relationship traversal
- Query parameters from schema
- OpenAPI specification
- Type-safe client generation

### Flexible Computed Fields
- Client-side computation (default, zero overhead)
- Server-side plugins (WASM/Lua/Python) when needed
- Language-agnostic contract
- Lazy evaluation

### UI Integration
- Components referenced in schema
- Type-safe props generation
- Server-side rendering support
- Multiple framework support (JSX, HTML, Svelte, Vue)

### Developer Experience
- CLI-driven workflow
- Hot reload on schema changes
- Auto-scaffolding of stubs
- Validation and type checking
- Migration generation

## Target Use Cases

### Perfect For
- Web applications with stable schemas
- Type-safe full-stack development
- Local-first applications (future WASM support)
- Embedded systems with schema known at compile-time
- Microservices with strong contracts
- Rapid prototyping with production-quality output

### Not Ideal For
- Highly dynamic schemas changing at runtime
- Analytics databases requiring ad-hoc queries on unknown schemas
- Systems requiring schema flexibility over performance

## Architecture Overview

```
schema.lang (declarative)
    ↓
Transpiler (parser + code generator)
    ↓
    ├─→ Rust Database Code
    │   ├─ Columnar storage implementation
    │   ├─ Type-safe query API
    │   ├─ REST API server
    │   └─ Migration system
    │
    ├─→ TypeScript Types & SDK
    │   ├─ Model types
    │   ├─ API client
    │   └─ Computed field contracts
    │
    ├─→ OpenAPI Specification
    │
    └─→ Component Stubs
        └─ Type-safe UI components
```

## Performance Characteristics

**Expected Benefits:**
- **Memory efficiency**: Columnar layout reduces memory footprint for sparse access
- **Query performance**: Direct memory access, SIMD operations on numeric columns
- **Zero deserialization**: Memory-mapped fixed-size types
- **Cache-friendly**: Sequential column access patterns
- **Compile-time optimization**: Rust compiler optimizes for specific schema

**Trade-offs:**
- Schema changes require recompilation
- Less flexible than runtime-generic databases
- Larger binary size (specialized code per schema)

## Development Phases

### Phase 1: Core Database (v1.0)
Foundation: Storage, types, basic queries, transpiler
Timeline: 6-8 months

### Phase 2: Full-Stack Integration (v2.0)
Developer experience: CLI, API generation, hot reload
Timeline: 4-6 months after v1

### Phase 3: AI-Powered Development (v3.0)
Future: AI agents implement components from schema annotations
Timeline: TBD (experimental)

## Project Structure

```
sinkdb/
├── schema-lang/          # DSL parser and AST
├── transpiler/           # Code generation engine
├── runtime/              # Rust runtime library
│   ├── storage/          # Columnar storage implementation
│   ├── query/            # Query execution
│   └── api/              # REST API framework
├── cli/                  # Developer tooling
├── examples/             # Example applications
└── docs/                 # Comprehensive documentation
```

## Getting Started (Future)

```bash
# Install CLI
cargo install sinkdb-cli

# Create new project
sinkdb init my-app
cd my-app

# Define schema
cat > schema.lang << EOF
User {
  id: +uuid
  email: ^&string
  name: string
}
EOF

# Generate and run
sinkdb dev
# Server running at http://localhost:3000
# API docs at http://localhost:3000/docs
```

## Philosophy

**Declarative Over Imperative**: Describe what you want, not how to build it.

**Compile-Time Over Runtime**: Catch errors before deployment, optimize for the specific schema.

**Type Safety Everywhere**: From database to UI, types flow through the entire stack.

**Convention Over Configuration**: Sensible defaults, escape hatches when needed.

**Performance By Design**: Architecture enables speed, not bolted on later.

## Related Projects

**Inspiration from:**
- Diesel (Rust ORM with compile-time queries)
- Prisma (Schema-first with excellent DX)
- Apache Arrow (Columnar memory format)
- PostgREST (Automatic API from schema)
- Rails (Convention over configuration)

**Differentiators:**
- Compile-time specialized code generation
- Columnar storage from the start
- Full-stack from a single schema
- Language-agnostic computed fields
- UI component integration

## License

TBD

## Contributing

TBD - Project in design phase

## Contact

TBD

---

## Latest Progress Summary

**Recent Completions:**
- ✅ Sprint 9: REST API Generation with full CRUD endpoints
- ✅ Sprint 10: TypeScript SDK Generation
- ✅ Sprint 11: Directives & Validation (Constraints)
- ✅ Sprint 12: Computed Fields
- ✅ Sprint 13: OpenAPI & Documentation
- ✅ 138 tests passing across all components
- ✅ 19 working examples demonstrating features

**Current Status:** Sprint 13 complete - Full API documentation generation implemented

See [SPRINT_PLAN.md](./SPRINT_PLAN.md) for detailed roadmap and [archive/sprint-summaries/](./archive/sprint-summaries/) for implementation details.

---

## Sprint 5-8 Implementation Highlights

### Sprint 8: Inline Structs & Fixed Arrays ✅ COMPLETE

**Key Features:**
- ✅ Inline struct definitions with deterministic fixed-size layout
- ✅ Fixed-size arrays (`[type; N]`) for compound data
- ✅ Nested struct support with proper alignment
- ✅ Zero-copy field access for optimal performance

See [archive/sprint-summaries/SPRINT8_SUMMARY.md](./archive/sprint-summaries/SPRINT8_SUMMARY.md) for details.

### Sprint 7: Write-Ahead Log & Durability ✅ COMPLETE

**Key Features:**
- ✅ WAL-based durability with crash recovery
- ✅ ACID transaction support (begin/commit/rollback)
- ✅ Configurable fsync policies (immediate, periodic, none)
- ✅ CRC32 checksums for corruption detection
- ✅ Automatic WAL replay on startup

See [archive/sprint-summaries/SPRINT7_SUMMARY.md](./archive/sprint-summaries/SPRINT7_SUMMARY.md) for details.

### Sprint 6: Multiple Models & Many-to-Many Relations ✅ COMPLETE

**Key Features:**
- ✅ Multiple model definitions in single schema
- ✅ Many-to-many relations with junction tables
- ✅ Automatic junction table generation
- ✅ Bidirectional relationship traversal
- ✅ Database struct managing all model storages

See [archive/sprint-summaries/SPRINT6_SUMMARY.md](./archive/sprint-summaries/SPRINT6_SUMMARY.md) for details.

### Sprint 5: Advanced Indexing ✅ COMPLETE

All Sprint 5 advanced indexing success criteria have been met:

**Composite Indexes:**
- ✅ Parse `@index(field1, field2, ...)` directive syntax
- ✅ Generate composite index storage structures
- ✅ Generate composite find methods (`find_by_X_and_Y`)
- ✅ Validate composite indexes reference existing fields
- ✅ Maintain composite indexes on insert/update/delete

**Range Queries & B-tree Indexes:**
- ✅ Automatic B-tree index for ordered types (numeric, timestamp)
- ✅ Hash index for unordered types (string, bool, uuid)
- ✅ Generate range query methods (_range, _gt, _gte, _lt, _lte)
- ✅ ordered-float integration for f64 in BTreeMap
- ✅ Full index maintenance across all CRUD operations

**Test Coverage:**
- ✅ 95 tests passing (6 new Sprint 5 tests)
- ✅ Comprehensive validation example

### Quick Start (Sprint 5)

```bash
# Run all tests
cargo test --lib

# Run Sprint 5 example
cargo run --example sprint5_advanced_indexing
```

### Example Schema (Sprint 5)

```
Product {
  id: +uuid
  name: string
  category: string
  price: ^f64           // B-tree indexed (range queries)
  stock: ^u32           // B-tree indexed (range queries)
  created_at: ^timestamp // B-tree indexed (range queries)

  @index(category, name) // Composite index
}
```

**Generated Query Methods:**
- `find_by_price_range(min, max)` - Range query
- `find_by_price_gt(min)` - Greater than
- `find_by_price_gte(min)` - Greater or equal
- `find_by_price_lt(max)` - Less than
- `find_by_price_lte(max)` - Less or equal
- `find_by_category_and_name(cat, name)` - Composite query

See [SPRINT5_INDEXES.md](./SPRINT5_INDEXES.md) for complete documentation.

---

## Sprint 4 + 4.1 Implementation: ✅ COMPLETE

All Sprint 4 success criteria have been met, plus Sprint 4.1 additions:

**Sprint 4 (Core):**
- ✅ Parse relation syntax: `[Post]`, `*User`, `?User`
- ✅ Generate foreign key columns (e.g., `author_id`)
- ✅ Foreign keys automatically indexed
- ✅ Schema validation prevents invalid references

**Sprint 4.1 (Database & Relations):**
- ✅ Database struct holding all model storages
- ✅ Runtime FK validation on insert
- ✅ Relation traversal methods (`db.user_posts(id)`)
- ✅ Reverse lookup methods (`db.post_author(id)`)
- ✅ Comprehensive test suite (89 tests total)

### Quick Start (Sprint 4.1)

```bash
# Run all tests
cargo test --lib

# Run Sprint 4 examples
cargo run --example sprint4_relations
cargo run --example sprint4_1_database
```

### Example Schema (Sprint 4)

```
User {
  id: +uuid
  email: ^&string
  posts: [Post]      // One-to-many (virtual)
}

Post {
  id: +uuid
  title: string
  author: *User      // Required FK (generates author_id)
}
```

See [SPRINT4_RELATIONS.md](./SPRINT4_RELATIONS.md) for complete documentation.

---

## Sprint 3 Implementation: ✅ COMPLETE

All Sprint 3 indexing and query success criteria have been met:

- ✅ Fast lookup by indexed fields (O(1) hash indexes)
- ✅ CRUD operations complete (insert, get, update, delete)
- ✅ Indexes rebuilt on database load (in-memory)
- ✅ Tombstones prevent deleted records from appearing
- ✅ Unique indexes (^&) and non-unique indexes (^)
- ✅ Automatic index maintenance on all operations
- ✅ Comprehensive test suite (74 tests total, 10 new for Sprint 3)
- ✅ Example demonstrating all features

### Quick Start (Sprint 3)

```bash
# Run all tests
cargo test --lib

# Run Sprint 3 example
cargo run --example sprint3_indexing_queries
```

### Example Schema (Sprint 3)

```
User {
  id: +uuid
  email: ^&string    // indexed + unique
  username: ^string  // indexed only
  age: u32
}
```

See [SPRINT3_INDEXING_QUERIES.md](./SPRINT3_INDEXING_QUERIES.md) for complete documentation.

---

## Sprint 2 Implementation: ✅ COMPLETE

All Sprint 2 persistence success criteria have been met:

- ✅ Memory-mapped columnar storage architecture
- ✅ Fixed-size column storage (u64) with direct file I/O
- ✅ Variable-length string storage with offset indices
- ✅ Manifest.json for metadata persistence
- ✅ Database survives restart
- ✅ Comprehensive test suite (8/8 passing)
- ✅ Example demonstrating persistence

### Quick Start (Sprint 2)

```bash
# Run persistence tests
cargo test --package sinkdb-storage

# Run persistence example
cargo run --example sprint2_persistence
```

See [SPRINT2_PERSISTENCE.md](./SPRINT2_PERSISTENCE.md) for complete documentation.

---

## Sprint 1 Implementation: ✅ COMPLETE

All Sprint 1 MVP success criteria have been met:

- ✅ Parse simple schema
- ✅ Generate compilable Rust code
- ✅ Insert users with auto-increment ID
- ✅ Enforce unique email constraint
- ✅ Retrieve users by ID
- ✅ All in-memory, no crashes

### Quick Start

1. **Run the code generator:**
   ```bash
   cargo run
   ```

   This creates `schema.sink` (if it doesn't exist) and generates code to `generated/database.rs`.

2. **Run the example:**
   ```bash
   cargo run --example basic
   ```

3. **Run tests:**
   ```bash
   cargo test
   ```

### Current Schema Syntax

```
User {
  id: +u64
  email: &string
}
```

**Symbols:**
- `+` - Auto-generate (auto-increment for u64)
- `&` - Unique constraint

**Types:**
- `u32`, `u64`, `string`

---

**Status**: Sprint 1 Complete - Active Development
**Version**: 0.1.0 (Sprint 1 MVP)
