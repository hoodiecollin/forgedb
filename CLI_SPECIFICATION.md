# TypeDB CLI Specification

## Overview

The `typedb` CLI is the primary developer interface for TypeDB. It handles project initialization, code generation, schema validation, development server, migrations, and more.

## Installation

```bash
# Via cargo
cargo install typedb-cli

# Via homebrew (future)
brew install typedb

# Via npm (wrapper, future)
npm install -g typedb-cli
```

## Project Structure

When you initialize a project, the CLI creates the following structure:

```
my-app/
├── schema.lang              # Your schema definition
├── typedb.toml             # Project configuration
│
├── generated/              # Auto-generated (never edit)
│   ├── db.rs              # Rust database implementation
│   ├── types.ts           # TypeScript types
│   ├── api.rs             # REST API server
│   └── openapi.yaml       # OpenAPI specification
│
├── src/                    # Your application code
│   ├── main.rs            # Rust entry point (if using)
│   ├── index.ts           # TypeScript entry point (if using)
│   │
│   ├── computed/          # Computed field implementations
│   │   ├── User.ts
│   │   └── Post.ts
│   │
│   └── views/             # UI components
│       ├── components/
│       │   ├── UserCard.jsx
│       │   └── PostPreview.jsx
│       └── admin/
│           └── UserForm.jsx
│
├── migrations/             # Schema migrations
│   ├── 001_initial.sql
│   └── 002_add_posts.sql
│
└── data/                   # Database files (dev)
    ├── db/
    └── wal/
```

## Commands

### Global Flags

```bash
--verbose, -v         # Verbose output
--quiet, -q          # Suppress output
--config, -c <path>  # Path to typedb.toml (default: ./typedb.toml)
--help, -h           # Show help
--version, -V        # Show version
```

---

## `typedb init`

Initialize a new TypeDB project.

### Usage

```bash
typedb init <project-name> [options]
```

### Options

```bash
--template <name>    # Use a template (blog, ecommerce, todo, blank)
--rust              # Include Rust backend
--typescript        # Include TypeScript frontend (default)
--full-stack        # Both Rust + TypeScript (default)
--api-only          # Just generate API, no frontend
```

### Examples

```bash
# Interactive mode
typedb init my-app

# With template
typedb init my-blog --template blog

# API only
typedb init my-api --api-only

# Rust backend only
typedb init my-service --rust --no-typescript
```

### What It Does

1. Creates project directory
2. Generates initial schema from template or blank
3. Creates config file (`typedb.toml`)
4. Runs initial code generation
5. Installs dependencies (if applicable)
6. Initializes git repository

### Output

```
✨ Creating project: my-app
📄 Created schema.lang
⚙️  Created typedb.toml
🔨 Generating code...
  ✓ Generated db.rs
  ✓ Generated types.ts
  ✓ Generated api.rs
  ✓ Generated openapi.yaml
📦 Installing dependencies...
✓ Done! Run 'cd my-app && typedb dev' to start
```

---

## `typedb dev`

Start development server with hot reload.

### Usage

```bash
typedb dev [options]
```

### Options

```bash
--port, -p <port>       # HTTP port (default: 3000)
--host <host>           # Bind address (default: 127.0.0.1)
--watch <path>          # Additional paths to watch
--no-hot-reload         # Disable hot reload
--no-browser           # Don't open browser
--api-only             # Start API server only
--db-path <path>       # Database location (default: ./data/db)
```

### Examples

```bash
# Basic dev mode
typedb dev

# Custom port
typedb dev --port 8080

# Watch additional directories
typedb dev --watch ./lib

# API only (no UI)
typedb dev --api-only
```

### What It Does

1. Watches `schema.lang` for changes
2. On change:
   - Parses and validates schema
   - Regenerates code (Rust + TypeScript)
   - Creates stubs for new computed fields / components
   - Recompiles and hot reloads server
3. Starts HTTP API server
4. Opens browser to API documentation
5. Displays logs and errors

### Output

```
🚀 TypeDB Dev Server

Watching: schema.lang
API Server: http://localhost:3000
API Docs: http://localhost:3000/docs
Database: ./data/db

[12:34:56] ✓ Schema validated
[12:34:56] ✓ Generated code
[12:34:57] ✓ Server started

Ready! Edit schema.lang to see changes.

[12:35:10] 📝 Schema changed...
[12:35:10] ⚠  New computed field: User.fullName
           Created stub: src/computed/User.ts
[12:35:10] ✓ Recompiled
[12:35:10] 🔄 Hot reloaded

[12:35:15] → GET /api/users?age>25 (3.2ms)
[12:35:16] → POST /api/users (5.1ms)
```

---

## `typedb generate`

Generate code from schema without starting dev server.

### Usage

```bash
typedb generate [targets] [options]
```

### Targets

```bash
all          # Generate everything (default)
rust         # Generate Rust code only
typescript   # Generate TypeScript only
api          # Generate API server
openapi      # Generate OpenAPI spec
migrations   # Generate migration from schema diff
stubs        # Generate missing computed/component stubs
```

### Options

```bash
--check             # Verify nothing needs regeneration (CI mode)
--output <dir>      # Output directory (default: ./generated)
--force            # Regenerate even if up-to-date
```

### Examples

```bash
# Generate everything
typedb generate

# TypeScript types only
typedb generate typescript

# Check if generation is needed (CI)
typedb generate --check

# Force regeneration
typedb generate --force
```

### What It Does

1. Parses schema
2. Generates requested targets
3. Creates stubs for missing implementations
4. Formats generated code
5. Reports what changed

### Output

```
🔨 Generating code from schema.lang

✓ Parsed schema (42 models, 156 fields)
✓ Generated db.rs (12,543 lines)
✓ Generated types.ts (3,421 lines)
✓ Generated api.rs (8,932 lines)
✓ Generated openapi.yaml (2,134 lines)

⚠  Missing implementations:
   - src/computed/User.ts: fullName
   - src/views/Dashboard.jsx

Run 'typedb generate stubs' to create them.

✓ Done in 1.2s
```

---

## `typedb validate`

Validate schema and check for missing implementations.

### Usage

```bash
typedb validate [options]
```

### Options

```bash
--strict            # Fail on unimplemented computed/views
--schema-only       # Only validate schema syntax
--implementations   # Check computed field implementations
--components        # Check UI component files
```

### Examples

```bash
# Validate everything
typedb validate

# Schema only
typedb validate --schema-only

# Strict mode (fail on missing impls)
typedb validate --strict
```

### What It Does

1. Parses and validates schema
2. Checks for:
   - Syntax errors
   - Semantic errors (invalid types, relations, etc.)
   - Missing computed field implementations
   - Missing component files
   - Unused files

### Output

```
🔍 Validating project...

✓ Schema syntax valid
✓ Semantic validation passed
  - 42 models
  - 156 fields
  - 23 relations
  - 8 computed fields

❌ Missing implementations:
   - User.fullName (src/computed/User.ts)
   - Post.readTime (src/computed/Post.ts)

❌ Referenced components not found:
   - jsx://views/Dashboard.jsx

⚠  Unused files (safe to delete):
   - src/views/OldComponent.jsx
   - src/computed/DeletedModel.ts

Run 'typedb generate stubs' to create missing files.

Validation failed with 4 errors.
```

---

## `typedb migrate`

Manage schema migrations.

### Usage

```bash
typedb migrate <command> [options]
```

### Subcommands

```bash
create <name>       # Create new migration from schema diff
up                  # Apply pending migrations
down [n]            # Rollback n migrations (default: 1)
status              # Show migration status
history             # Show migration history
rollback <version>  # Rollback to specific version
```

### `typedb migrate create`

```bash
typedb migrate create "add user profiles" [options]

Options:
  --auto              # Auto-generate migration (if safe)
  --template          # Create empty migration template
  --dry-run          # Show what would be generated
```

**Output:**
```
📝 Creating migration: add_user_profiles

✓ Detected schema changes:
  + User.bio: string?
  + User.avatar: string?
  + Profile model (new)

✓ Generated migration: migrations/003_add_user_profiles.sql
✓ Generated rollback: migrations/003_add_user_profiles.down.sql

⚠  Review migration before running 'typedb migrate up'
```

### `typedb migrate up`

```bash
typedb migrate up [options]

Options:
  --to <version>      # Migrate to specific version
  --dry-run          # Show what would happen
  --force            # Skip confirmation
```

**Output:**
```
🔄 Applying migrations...

Pending migrations:
  001_initial.sql
  002_add_posts.sql
  003_add_user_profiles.sql

Apply these migrations? (y/N): y

✓ 001_initial.sql (234ms)
✓ 002_add_posts.sql (123ms)
✓ 003_add_user_profiles.sql (89ms)

✓ Applied 3 migrations in 446ms
```

### `typedb migrate down`

```bash
typedb migrate down [n] [options]

Options:
  --force            # Skip confirmation
```

**Output:**
```
🔄 Rolling back migrations...

Will rollback:
  003_add_user_profiles.sql

Continue? (y/N): y

✓ Rolled back 003_add_user_profiles.sql

Current version: 002
```

### `typedb migrate status`

**Output:**
```
📊 Migration Status

Current version: 002
Pending migrations: 1

Applied:
  ✓ 001_initial.sql (2024-10-01 10:30:00)
  ✓ 002_add_posts.sql (2024-10-05 14:22:00)

Pending:
  - 003_add_user_profiles.sql

Run 'typedb migrate up' to apply pending migrations.
```

---

## `typedb build`

Build production-ready artifacts.

### Usage

```bash
typedb build [options]
```

### Options

```bash
--release           # Build with optimizations (default)
--target <target>   # Build target (native, wasm, both)
--output <dir>      # Output directory (default: ./dist)
--no-api           # Skip API server build
--no-db            # Skip database build
```

### Examples

```bash
# Production build
typedb build --release

# WASM target
typedb build --target wasm

# API server only
typedb build --no-db
```

### What It Does

1. Validates schema
2. Generates production code
3. Compiles Rust with optimizations
4. Bundles TypeScript
5. Creates deployment artifacts

### Output

```
🔨 Building production artifacts...

✓ Schema validated
✓ Generated code
✓ Compiled database (release)
✓ Compiled API server (release)
✓ Bundled TypeScript

📦 Output:
  dist/
  ├── typedb-api (binary)
  ├── libtypedb.a (static library)
  └── typedb.wasm (if --target wasm)

✓ Build complete in 45.2s
```

---

## `typedb inspect`

Inspect and introspect the project.

### Usage

```bash
typedb inspect <target> [options]
```

### Targets

```bash
schema          # Show parsed schema
types           # Show type information
routes          # Show generated API routes
computed        # Show computed field contracts
components      # Show UI component contracts
stats           # Show project statistics
```

### Examples

```bash
# Show schema
typedb inspect schema

# Show API routes
typedb inspect routes

# Show statistics
typedb inspect stats
```

### Output Examples

**`typedb inspect routes`**

```
🔍 Generated API Routes

Base URL: /api

Users:
  GET    /api/users
  GET    /api/users/{id}
  POST   /api/users
  PUT    /api/users/{id}
  PATCH  /api/users/{id}
  DELETE /api/users/{id}
  GET    /api/users/{id}/posts

Posts:
  GET    /api/posts
  GET    /api/posts/{id}
  POST   /api/posts
  PUT    /api/posts/{id}
  DELETE /api/posts/{id}

Total: 11 routes
```

**`typedb inspect stats`**

```
📊 Project Statistics

Schema:
  Models: 42
  Fields: 156
  Relations: 23
  Computed Fields: 8
  UI Components: 12

Generated Code:
  Rust: 25,432 lines
  TypeScript: 8,921 lines
  Total: 34,353 lines

Database:
  Tables: 42
  Columns: 156
  Indices: 31
  Size: 245 MB

Implementations:
  Computed: 8/8 (100%)
  Components: 10/12 (83%)
```

---

## `typedb test`

Run tests for generated code and implementations.

### Usage

```bash
typedb test [pattern] [options]
```

### Options

```bash
--unit              # Run unit tests
--integration       # Run integration tests
--performance       # Run performance benchmarks
--coverage          # Generate coverage report
```

### Examples

```bash
# Run all tests
typedb test

# Run unit tests only
typedb test --unit

# Run specific test file
typedb test user_test

# With coverage
typedb test --coverage
```

---

## `typedb plugin`

Manage computed field plugins (WASM, Lua, etc.).

### Usage

```bash
typedb plugin <command> [options]
```

### Subcommands

```bash
add <path>          # Add a plugin
remove <name>       # Remove a plugin
list                # List installed plugins
info <name>         # Show plugin info
```

### Examples

```bash
# Add WASM plugin
typedb plugin add ./credit_score.wasm

# List plugins
typedb plugin list

# Remove plugin
typedb plugin remove credit_score
```

---

## `typedb benchmark`

Run performance benchmarks.

### Usage

```bash
typedb benchmark [options]
```

### Options

```bash
--suite <name>      # Run specific benchmark suite
--compare <db>      # Compare against another database
--output <file>     # Save results to file
--warmup <n>        # Warmup iterations (default: 3)
--iterations <n>    # Benchmark iterations (default: 10)
```

### Examples

```bash
# Run all benchmarks
typedb benchmark

# Compare with SQLite
typedb benchmark --compare sqlite

# Specific suite
typedb benchmark --suite insert
```

### Output

```
🏃 Running Benchmarks...

Insert (1M rows):
  TypeDB:    2.3s  ████████████████████ (baseline)
  SQLite:    3.8s  █████████████████████████████████ (+65%)

Query (scan 1M rows, filter):
  TypeDB:    89ms  █████ (baseline)
  DuckDB:    67ms  ████ (-25%)
  SQLite:   234ms  ████████████ (+163%)

Join (10k users × 100k posts):
  TypeDB:    42ms  ████████████████ (baseline)
  PostgreSQL: 38ms  ██████████████ (-10%)

Memory (1M rows):
  TypeDB:    85 MB
  SQLite:   142 MB (+67%)
```

---

## Configuration File: `typedb.toml`

```toml
[project]
name = "my-app"
version = "0.1.0"

[database]
path = "./data/db"          # Database location
wal_path = "./data/wal"     # WAL location
page_size = 4096            # Page size in bytes
compaction_threshold = 0.20 # Compact when >20% deleted

[api]
port = 3000
host = "127.0.0.1"
cors_origins = ["http://localhost:5173"]
rate_limit = 1000           # Requests per minute

[dev]
hot_reload = true
watch_paths = ["schema.lang", "src/"]
browser = true              # Auto-open browser

[codegen]
rust_output = "./generated"
typescript_output = "./generated"
format_rust = true          # Run rustfmt
format_typescript = true    # Run prettier

[plugins]
computed_fields = [
  { name = "credit_score", path = "./plugins/credit_score.wasm" }
]

[performance]
cache_size = "1GB"          # Cache size
thread_pool = 4             # Worker threads
enable_simd = true          # Use SIMD instructions
```

---

## Environment Variables

```bash
TYPEDB_HOME          # TypeDB installation directory
TYPEDB_CONFIG        # Path to typedb.toml (overrides --config)
TYPEDB_LOG           # Log level (error, warn, info, debug, trace)
TYPEDB_NO_COLOR     # Disable colored output
RUST_LOG            # Rust logging (for internal debugging)
```

---

## Exit Codes

```
0   Success
1   General error
2   Schema validation error
3   Code generation error
4   Compilation error
5   Runtime error
10  Configuration error
11  File not found
12  Permission denied
```

---

## Scripting & CI/CD

### GitHub Actions Example

```yaml
name: TypeDB CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install TypeDB CLI
        run: cargo install typedb-cli
      
      - name: Validate Schema
        run: typedb validate --strict
      
      - name: Generate Code
        run: typedb generate --check
      
      - name: Run Tests
        run: typedb test --coverage
      
      - name: Build
        run: typedb build --release
```

---

## Troubleshooting

### Common Issues

**Schema changes not reloading:**
```bash
# Force regeneration
typedb generate --force

# Restart dev server
typedb dev
```

**Port already in use:**
```bash
# Use different port
typedb dev --port 8080
```

**Generated code doesn't compile:**
```bash
# Check for schema errors
typedb validate

# Clean and regenerate
rm -rf generated/
typedb generate
```

**Database corruption:**
```bash
# Verify database
typedb inspect stats

# Restore from WAL
typedb recover

# Last resort: rebuild from migrations
typedb migrate down --all
typedb migrate up
```

---

## Future Commands (v2+)

```bash
typedb export        # Export data (JSON, CSV, SQL)
typedb import        # Import data
typedb backup        # Create backup
typedb restore       # Restore from backup
typedb replicate     # Setup replication
typedb ai            # AI assistant (v3)
```

---

**Document Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Specification
