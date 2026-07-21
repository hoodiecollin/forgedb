# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

ForgeDB is a schema-first database framework that generates type-safe database implementations, APIs, and SDKs from declarative schemas. The system transpiles `.forge` schema files into optimized Rust code with columnar storage, REST APIs, TypeScript SDKs, and React component stubs.

**Core Philosophy**: Single source of truth - one schema file defines database, API, types, and UI contracts with end-to-end type safety.

## Common Development Commands

### Building & Testing

```bash
# Run all library tests
cargo test --lib

# Run specific crate tests
cargo test -p forgedb-storage
cargo test -p forgedb-types

# Run integration tests
cargo test --test '*'

# Build the CLI
cargo build --release -p forgedb-cli

# Build everything
cargo build --all
```

### Development Workflow

```bash
# Validate a schema file
cargo run -p forgedb-cli -- validate schema.forge

# Generate code from schema
cargo run -p forgedb-cli -- generate all --output ./generated

# Watch schema and auto-regenerate on changes
cargo run -p forgedb-cli -- dev --schema schema.forge

# Start the server (Rust API + Bun runtime)
cargo run -p forgedb-cli -- serve --port 3000

# Create and run migrations
cargo run -p forgedb-cli -- migrate create "add_users_table" --auto
cargo run -p forgedb-cli -- migrate up
```

### NPM Package Development

```bash
# Build the NPM package (requires built Rust binary)
cd npm-package
bun install
bun run build

# Test the CLI wrapper
bun run test
```

### VSCode Extension Development

```bash
cd vscode-forgedb
bun install
bun run compile

# Package extension
bun run package
```

## Architecture Overview

ForgeDB is a Rust workspace with modular crates:

### Core Crates

- **`forgedb` (root)**: Main library with schema parser, AST, and code generators
  - `src/lexer.rs` - Tokenization
  - `src/parser.rs` - Schema parsing
  - `src/ast.rs` - Abstract syntax tree
  - `src/codegen.rs` - Rust database code generation (largest file ~4000 lines)
  - `src/typescript_codegen.rs` - TypeScript SDK generation
  - `src/api_codegen.rs` - REST API generation
  - `src/openapi_codegen.rs` - OpenAPI spec generation
  - `src/component_stubs.rs` - React component stub generation

### Supporting Crates

- **`storage`**: Columnar storage engine with memory-mapped files
- **`types`**: Core type system (uuid, timestamp, primitives)
- **`validation`**: Schema validation and constraints
- **`wal`**: Write-ahead logging for durability
- **`migrations`**: Schema migration system
- **`compaction`**: Database compaction to reclaim space
- **`fulltext`**: Full-text search indexing
- **`query-optimization`**: Query planning and optimization
- **`http-server`**: Axum-based REST API server
- **`crud-api`**: CRUD operation implementations
- **`query-params`**: Query parameter parsing
- **`cli`**: Command-line interface (uses `clap`)
- **`watcher`**: File watching for hot reload
- **`lsp-server`**: Language Server Protocol for IDE support
- **`ffi`**: Foreign function interface for Bun/Node.js integration

### Generated Code Flow

```
schema.forge
    ↓ (lexer → parser)
AST (Abstract Syntax Tree)
    ↓ (validation)
Semantic Model
    ↓ (codegen)
    ├─→ Rust database.rs (columnar storage + query API)
    ├─→ TypeScript types.ts (type definitions + SDK)
    ├─→ openapi.yaml (REST API specification)
    └─→ React component stubs (TSX/JSX files)
```

## Schema Language Key Concepts

### Type System

- **Primitives**: `u32`, `u64`, `i32`, `i64`, `f64`, `bool`, `string`
- **Special**: `uuid`, `timestamp`, `char(n)`, `text`
- **Relations**: `[Model]` (one-to-many), `Model` (foreign key)
- **Arrays**: `[type; size]` for fixed-size arrays
- **Inline structs**: `Address { street: string, city: string }`

### Symbols & Directives

- `+` - Auto-generate on create (e.g., `id: +uuid`)
- `~` - Auto-update on modify (e.g., `updated_at: ~timestamp`)
- `^` - Create index (e.g., `email: ^string`)
- `&` - Unique constraint (e.g., `username: ^&string`)
- `?` - Nullable (e.g., `bio: string?`)
- `*` - Required foreign key (e.g., `author: *User`)
- `@` - Directives (e.g., `@min(5)`, `@max(100)`, `@email`, `@pattern(regex)`)

### Component Integration

- `tsx://path/to/Component` - React TypeScript component
- `jsx://path/to/Component` - React JavaScript component
- `api://path/to/handler` - API route handler
- `@relations(field1, field2)` or `@relations(*)` - Include related data in component props

## Storage Architecture

### Columnar Storage

ForgeDB uses a hybrid columnar storage model:

- **Fixed-size columns**: Memory-mapped for zero-copy access (`u64`, `f64`, `uuid`, `timestamp`, inline structs)
- **Variable-length columns**: Append-only with offset indices (`string`, `text`)
- **Relations**: Stored as foreign keys in fixed-size columns
- **Indexes**: Hash indexes for unique lookups, B-tree for range queries
- **Tombstones**: Bitmap for soft deletion (reclaimed during compaction)

### Data Directory Structure

```
data/
├── manifest.json       # Schema metadata
├── tombstones.bin     # Deletion bitmap
├── wal/              # Write-ahead log
├── fixed/            # Fixed-size columns
└── variable/         # Variable-length data
```

## Code Generation Patterns

### When Modifying Codegen

1. **Understand the AST first**: All code generation starts from parsed AST in `src/ast.rs`
2. **Use templates carefully**: Most generators use string templates with careful escaping
3. **Maintain type safety**: Generated code must be type-safe in target language
4. **Test thoroughly**: Add tests in `tests/` or `crates/tests/src/`
5. **Update multiple generators**: Changes often affect Rust, TypeScript, and OpenAPI generators

### Key Code Generation Functions

- `generate_rust_code()` in `src/codegen.rs` - Main Rust database implementation
- `generate_typescript()` in `src/typescript_codegen.rs` - TypeScript SDK
- `generate_openapi_spec()` in `src/openapi_codegen.rs` - OpenAPI specification
- `generate_component_stubs()` in `src/component_stubs.rs` - React components

## Testing Patterns

### Test Organization

- **Unit tests**: In each crate's `src/` with `#[cfg(test)]` modules
- **Integration tests**: In `tests/` directory at root
- **Crate-specific tests**: In `crates/tests/src/`

### Running Specific Tests

```bash
# Run tests for a specific file
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run single test
cargo test test_specific_function -- --exact

# Run tests in a specific crate
cargo test -p forgedb-storage
```

## FFI Integration (Bun/Node.js)

ForgeDB exposes a C-compatible FFI for JavaScript runtimes:

- **Crate**: `crates/ffi/`
- **Pattern**: Rust functions exposed with `#[no_mangle]` and `extern "C"`
- **Memory management**: Caller owns returned pointers, must call `free_*` functions
- **Async**: FFI is synchronous; async handled in TypeScript wrapper

### FFI Development

When adding FFI functions:

1. Add function to `crates/ffi/src/lib.rs` with `#[no_mangle]` and `extern "C"`
2. Use `CString` for string arguments
3. Return raw pointers or primitive types
4. Add corresponding free function for allocated memory
5. Update TypeScript bindings in generated code

## NPM Package

The `npm-package/` directory contains the NPM distribution:

- **Structure**: Wraps Rust CLI binary for Node.js users
- **Binary distribution**: Platform-specific binaries bundled as optional dependencies
- **Post-install**: Downloads/extracts correct binary for platform
- **CLI wrapper**: `bin/forgedb.js` spawns Rust binary with arguments

## Development Tips

### Common Issues

- **Schema parse errors**: Check `src/parser.rs` for grammar rules
- **Code generation fails**: Validate AST structure in `src/ast.rs`
- **Storage corruption**: WAL may need recovery, check `crates/wal/`
- **FFI crashes**: Ensure proper memory management and null checks

### Performance Considerations

- **Columnar scans**: Optimized for sequential access, not random access
- **Indexing**: Always index fields used in WHERE clauses
- **Computed fields**: Default to client-side for zero overhead
- **Compaction**: Run periodically to reclaim space from deleted records

### Migration Workflow

Schema changes require migrations:

1. Modify `schema.forge`
2. Run `forgedb migrate create "description" --auto`
3. Review generated migration in `migrations/`
4. Apply with `forgedb migrate up`
5. Regenerate code with `forgedb generate all`

## VSCode Extension

The `vscode-forgedb/` directory contains the IDE extension:

- **Language**: TypeScript
- **Features**: Syntax highlighting, LSP client, schema validation
- **LSP Server**: Implemented in `crates/lsp-server/`
- **Build**: Uses TypeScript compiler (`tsc`)
- **Package**: Creates `.vsix` with `vsce package`

## Related Documentation

- **README.md**: High-level project overview and philosophy
- **INDEX.md**: Comprehensive documentation index
- **TASKS.md**: Future enhancement ideas (not prioritized)
- **NPM_PACKAGE_PLAN.md**: NPM packaging implementation details
- **archive/sprint-summaries/**: Historical sprint completion notes
- **docs/**: Additional technical documentation

## User Preferences (from Rules)

- **Runtime**: Prefer Bun over Node.js
- **Package manager**: Use Bun for package management
- **Language**: Always use TypeScript (never JavaScript)
- **Tool IDs**: Use kebab-case format
- **Database patterns**: Prefer separate tables over JSONB columns for complex types
- **Drizzle queries**: End with `.$dynamic()` when clauses are added conditionally
- **AI SDK tools**: Use `tool()` helper function from AI SDK for type inference
