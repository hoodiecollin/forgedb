<div align="center">

# ForgeDB — Type-Safe, Schema-First Full-Stack Database Generator

</div>

## Overview

ForgeDB is an **application-database generator**: a declarative `.forge` schema is transpiled at
compile time into tailored Rust database code, a TypeScript SDK, a REST API, and UI component
stubs. It is a code-generation tool, **not** a runtime ORM or query engine — the schema is a
compile-time input to generation, never a runtime input to a generic engine. The generated code
uses columnar storage and is specialized per schema, so there is no runtime overhead from generic
data structures.

Everything in this repository is open source under `MIT OR Apache-2.0`; see
[`docs/OPEN_CORE.md`](./docs/OPEN_CORE.md) for the open-core boundary.

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

### Computed Fields (contract-level)
- Declared in the schema as a language-agnostic contract
- Client-side computation by default (zero storage overhead)
- Expression-backed generated getters are planned (see the roadmap)

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
- Local-first applications (browser read-replica via WASM)
- Embedded systems with schema known at compile-time
- Microservices with strong contracts
- Rapid prototyping with production-quality output

### Not Ideal For
- Highly dynamic schemas changing at runtime
- Analytics databases requiring ad-hoc queries on unknown schemas
- Systems requiring schema flexibility over performance

## Architecture Overview

```
schema.forge (declarative)
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

## Project Structure

```
forgedb/
├── src/                  # The `forgedb` CLI (main.rs + one module per subcommand)
├── crates/               # Focused crates: schema-agnostic substrate + compiler internals
│   ├── storage-native/   # Columnar storage engine (native)
│   ├── storage-web/      # Browser arena backend (WASM read-replica)
│   ├── wal/              # Write-Ahead Log
│   ├── parser/           # Schema parser (lexer → AST)
│   ├── codegen/          # Code generators
│   └── ...               # See docs/ARCHITECTURE.md and docs/PUBLIC_CRATES.md
├── docs/                 # Documentation (see the index below)
├── examples/             # Worked example schemas across many domains
├── apps/                 # Inspector (Tauri) + marketing/docs website
└── tests/                # Integration tests
```

## Getting Started

### Build and generate from a schema

```bash
# Clone the repository
git clone https://github.com/hoodiecollin/forgedb
cd forgedb

# Build the workspace
cargo build --workspace

# Generate code from a schema (Rust, TypeScript, API, and stubs).
# `generate` auto-discovers `schema.forge` in the current directory.
cargo run -- generate all --output ./generated

# Explore the generated code
ls ./generated
```

See [`examples/`](./examples/) for worked schemas across many domains, and
[docs/GETTING_STARTED.md](./docs/GETTING_STARTED.md) for the full loop with verified output.

### Install the CLI and scaffold a project

```bash
# Install from crates.io
cargo install forgedb

# Create a new project
forgedb init my-app
cd my-app

# Define a schema
cat > schema.forge << EOF
User {
  id: +uuid
  email: ^&string
  name: string
}
EOF

# Generate, build, and run the dev server
forgedb dev
```

See [docs/INSTALL.md](./docs/INSTALL.md) for every install path (prebuilt binaries, `--git`,
from-clone) and the substrate version matrix.

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

Dual-licensed under either [MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE) at your option.

## Documentation

Comprehensive documentation is available in the [`docs/`](./docs/) directory.

**Start here:**

- **[Getting Started](./docs/GETTING_STARTED.md)** - Install → scaffold → generate → build → serve → typed SDK
- **[Schema Language Reference](./docs/SCHEMA.md)** - The complete, parser-verified `.forge` reference
- **[What v1 Is — and Isn't](./docs/WHAT_V1_IS.md)** - Honest guarantees and limits of v1
- **[Installing](./docs/INSTALL.md)** - Every install path + the substrate version matrix

**Operating:**

- **[Deployment](./docs/DEPLOYMENT.md)** - Containers, env config, ops routes, multi-tenancy, JWT
- **[Migrations](./docs/MIGRATIONS.md)** - How schema changes affect existing data
- **[Versioning & Stability](./docs/SEMVER.md)** - Compatibility policy across the four surfaces

**Internals & contributing:**

- **[Architecture](./docs/ARCHITECTURE.md)** - System design, component architecture, and design decisions
- **[Public Crates](./docs/PUBLIC_CRATES.md)** - Runtime library guide and API documentation
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

## Status

ForgeDB is in **early development (0.1.x)** and not yet production-ready. The workspace builds on
Rust 2024 (pinned via `rust-toolchain.toml`).

Implemented and working:
- Schema parser (lexer → AST) and validation
- Columnar storage engine, WAL, in-process compaction
- Crash-safe durable writes, MVCC transactions + multi-process write coordination
- Code generation: Rust database, TypeScript SDK, REST API (+ OpenAPI 3.1), React/route stubs
- Secondary indexes, relation traversal, snapshot reads, live queries, backup/restore
- Multi-tenancy (verify-only JWT), schema migrations, browser read-replica (WASM)
- LSP server + VS Code extension, language bindings (Python / Node / Bun)

For the honest scope — what v1 guarantees and what it defers — see
[docs/WHAT_V1_IS.md](./docs/WHAT_V1_IS.md) and [docs/V1_ROADMAP.md](./docs/V1_ROADMAP.md).

---

## Quick Start

```bash
# Run all tests
cargo test --workspace

# Generate all outputs from the schema in the current directory
cargo run -- generate all --output ./generated
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

  // UI components
  profileCard: tsx://components/user/ProfileCard @relations(posts)
  avatar: jsx://components/user/Avatar

  // API routes
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

See [`examples/`](./examples/) for worked schemas across many domains.

---

**Status**: Early development — see [Status](#status) above.
**Version**: 0.1.x
