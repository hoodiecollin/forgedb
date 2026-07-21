<div align="center">
  <img src="assets/tmp-logo.png" alt="ForgeDB Logo" width="200">
</div>

# ForgeDB - Type-Safe, Schema-First Full-Stack Database Framework

## Executive Summary

ForgeDB is a revolutionary database system that uses a declarative schema language to generate a complete full-stack application: database, type-safe APIs, UI components, and developer tooling. The system transpiles schema definitions into highly optimized Rust code with columnar storage, providing exceptional performance through compile-time optimization while maintaining perfect type safety across the entire stack.

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
forgedb/
├── crates/               # 15 focused crates (public + internal)
│   ├── storage/          # Columnar storage engine
│   ├── wal/              # Write-Ahead Log
│   ├── http-server/      # HTTP server infrastructure
│   ├── parser/           # Schema parser
│   └── ...               # See docs/ARCHITECTURE.md
├── docs/                 # Comprehensive documentation
│   ├── ARCHITECTURE.md   # System design
│   ├── PUBLIC_CRATES.md  # Runtime library guide
│   ├── INTERNAL_CRATES.md # Tooling guide
│   ├── CONTRIBUTING.md   # Contribution guidelines
│   ├── DEVELOPMENT.md    # Development setup
│   └── PUBLISHING.md     # Release process
├── cli/                  # Developer tooling (src/main.rs)
├── examples/             # Example applications
└── tests/                # Integration tests
```

## Getting Started

### Try the Comprehensive Example

```bash
# Clone the repository
git clone https://github.com/yourusername/forgedb
cd forgedb

# Run the comprehensive blog platform example
cargo run --example blog_platform

# Explore the generated code
ls generated/blog_platform/
```

This example demonstrates **all ForgeDB features** including:
- Multi-model schemas with relations
- All data types and indexes
- REST API generation
- TypeScript SDK
- OpenAPI documentation

See [examples/README.md](./examples/README.md) for detailed usage.

### Future: CLI Usage

```bash
# Install CLI (coming soon)
cargo install forgedb-cli

# Create new project
forgedb init my-app
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
forgedb dev
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

## Documentation

Comprehensive documentation is available in the [`docs/`](./docs/) directory:

- **[Architecture](./docs/ARCHITECTURE.md)** - System design, component architecture, and design decisions
- **[Public Crates](./docs/PUBLIC_CRATES.md)** - Runtime library guide and API documentation
- **[Internal Crates](./docs/INTERNAL_CRATES.md)** - Tooling and code generation pipeline
- **[Contributing](./docs/CONTRIBUTING.md)** - Contribution guidelines and code of conduct
- **[Development](./docs/DEVELOPMENT.md)** - Development environment setup and workflow
- **[Publishing](./docs/PUBLISHING.md)** - Release process and version management

## Contributing

We welcome contributions! Please read our [Contributing Guide](./docs/CONTRIBUTING.md) to get started.

Key areas where we need help:
- Bug fixes and testing
- Documentation improvements
- Code examples
- Performance optimization

## Contact

- **GitHub Issues**: [Report bugs or request features](https://github.com/hoodiecollin/forgedb/issues)
- **GitHub Discussions**: [Ask questions or share ideas](https://github.com/hoodiecollin/forgedb/discussions)

---

## Latest Progress Summary

**Recent Completions:**
- ✅ Sprint 9: REST API Generation with full CRUD endpoints
- ✅ Sprint 10: TypeScript SDK Generation
- ✅ Sprint 11: Directives & Validation (Constraints)
- ✅ Sprint 12: Computed Fields
- ✅ Sprint 13: OpenAPI & Documentation
- ✅ Sprint 14: Query Optimization & Planning
- ✅ Sprint 15: Log Compaction
- ✅ Sprint 16: Schema Migrations
- ✅ Sprint 17: UI Component Integration
- ✅ Sprint 18: Full-Text Search
- ✅ Sprint 20: Production Readiness
- ✅ Sprint 21: Syntax Highlighting
- ✅ Sprint 22: Language Server Protocol (LSP)
- ✅ Sprint 23: VSCode Extension
- ✅ 118 tests passing (115 unit + 3 integration)
- ✅ Comprehensive examples showcasing all features

**Current Status:** Sprint 17 complete - UI Component Integration with TSX/JSX/API route generation

**Latest Features:**
- Component fields in schema (`tsx://`, `jsx://`, `api://`)
- `@relations` directive for component props
- TypeScript component props type generation
- React component stub generation (Next.js App Router style)
- API route handler generation
- Virtual field support for components

See [SPRINT_PLAN.md](./SPRINT_PLAN.md) for detailed roadmap, [SPRINT17_SUMMARY.md](./SPRINT17_SUMMARY.md) for Sprint 17 details, and [archive/sprint-summaries/](./archive/sprint-summaries/) for all sprint summaries.

---

## Quick Start

```bash
# Run all tests
cargo test --lib

# Run the comprehensive example showcasing all features
cargo run --example blog_platform
```

## Example Schema

```
User {
  id: +uuid                    // Auto-generated primary key
  email: ^&string              // Unique indexed field
  username: ^&string
  display_name: string
  created_at: ^timestamp       // Indexed for range queries
  posts: [Post]                // One-to-many relation
  liked_posts: [Post]          // Many-to-many relation

  // UI Components (Sprint 17)
  profileCard: tsx://components/user/ProfileCard @relations(posts)
  avatar: jsx://components/user/Avatar

  // API Routes (Sprint 17)
  verifyEmail: api://routes/user/verify

  @index(created_at, username) // Composite index
  @pattern(email, "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$")
}

Post {
  id: +uuid
  title: ^string               // Indexed for search
  content: string
  author: *User                // Required foreign key
  keywords: [string; 10]       // Fixed-size array
  view_count: ^u64             // Indexed for range queries
  tags: [Tag]                  // Many-to-many

  // UI Component with all relations
  detailView: tsx://components/post/DetailView @relations(*)

  @index(author, created_at)
  @min(title, 5)
  @max(title, 200)
}
```

**Generated Query Methods:**
- `find_by_email(email)` - O(1) unique lookup
- `find_by_view_count_range(min, max)` - Range query
- `find_by_view_count_gt(min)` - Greater than
- `find_by_author_and_created_at(id, date)` - Composite index
- `post_tags.add_relation(post_id, tag_id)` - Many-to-many

See [examples/README.md](./examples/README.md) for complete usage guide and detailed feature documentation.

---

## Implementation Highlights (Sprints 1-13)

All core features have been implemented:

**✅ Sprint 1-4:** Core database, types, indexing, relations
**✅ Sprint 5-8:** Advanced indexing, validation, persistence, inline structs
**✅ Sprint 9-13:** REST API, TypeScript SDK, OpenAPI documentation

For detailed sprint documentation, see:
- [SPRINT_PLAN.md](./SPRINT_PLAN.md) - Feature roadmap
- [archive/sprint-summaries/](./archive/sprint-summaries/) - Implementation details
- [examples/README.md](./examples/README.md) - Comprehensive usage guide

---

**Status**: All Sprints Complete (1-13) - Production Ready
**Version**: 0.13.0
