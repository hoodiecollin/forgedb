# ForgeDB CLI Specification

## Overview

The `forgedb` CLI is the primary developer interface for ForgeDB. It handles project initialization, code generation, schema validation, development server, migrations, and more.

## Installation

```bash
# Via cargo
cargo install forgedb-cli

# Via homebrew (future)
brew install forgedb

# Via npm (wrapper, future)
npm install -g forgedb-cli
```

## Project Structure

When you initialize a project, the CLI creates the following structure:

```
my-app/
├── schema.lang              # Your schema definition
├── forgedb.toml             # Project configuration
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
--config, -c <path>  # Path to forgedb.toml (default: ./forgedb.toml)
--help, -h           # Show help
--version, -V        # Show version
```

---

## `forgedb init`

Initialize a new ForgeDB project.

### Usage

```bash
forgedb init <project-name> [options]
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
forgedb init my-app

# With template
forgedb init my-blog --template blog

# API only
forgedb init my-api --api-only

# Rust backend only
forgedb init my-service --rust --no-typescript
```

### What It Does

1. Creates project directory
2. Generates initial schema from template or blank
3. Creates config file (`forgedb.toml`)
4. Runs initial code generation
5. Installs dependencies (if applicable)
6. Initializes git repository

### Output

```
✨ Creating project: my-app
📄 Created schema.lang
⚙️  Created forgedb.toml
🔨 Generating code...
  ✓ Generated db.rs
  ✓ Generated types.ts
  ✓ Generated api.rs
  ✓ Generated openapi.yaml
📦 Installing dependencies...
✓ Done! Run 'cd my-app && forgedb dev' to start
```

---

## `forgedb dev`

Start development server with hot reload.

### Usage

```bash
forgedb dev [options]
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
forgedb dev

# Custom port
forgedb dev --port 8080

# Watch additional directories
forgedb dev --watch ./lib

# API only (no UI)
forgedb dev --api-only
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
🚀 ForgeDB Dev Server

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

## `forgedb generate`

Generate code from schema without starting dev server.

### Usage

```bash
forgedb generate [targets] [options]
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
forgedb generate

# TypeScript types only
forgedb generate typescript

# Check if generation is needed (CI)
forgedb generate --check

# Force regeneration
forgedb generate --force
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

Run 'forgedb generate stubs' to create them.

✓ Done in 1.2s
```

---

## `forgedb validate`

Validate schema and check for missing implementations.

### Usage

```bash
forgedb validate [options]
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
forgedb validate

# Schema only
forgedb validate --schema-only

# Strict mode (fail on missing impls)
forgedb validate --strict
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

Run 'forgedb generate stubs' to create missing files.

Validation failed with 4 errors.
```

---

## `forgedb migrate`

Manage schema migrations.

### Usage

```bash
forgedb migrate <command> [options]
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

### `forgedb migrate create`

```bash
forgedb migrate create "add user profiles" [options]

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

⚠  Review migration before running 'forgedb migrate up'
```

### `forgedb migrate up`

```bash
forgedb migrate up [options]

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

### `forgedb migrate down`

```bash
forgedb migrate down [n] [options]

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

### `forgedb migrate status`

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

Run 'forgedb migrate up' to apply pending migrations.
```

---

## `forgedb build`

Build production-ready artifacts.

### Usage

```bash
forgedb build [options]
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
forgedb build --release

# WASM target
forgedb build --target wasm

# API server only
forgedb build --no-db
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
  ├── forgedb-api (binary)
  ├── libforgedb.a (static library)
  └── forgedb.wasm (if --target wasm)

✓ Build complete in 45.2s
```

---

## `forgedb inspect`

Inspect and introspect the project.

### Usage

```bash
forgedb inspect <target> [options]
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
forgedb inspect schema

# Show API routes
forgedb inspect routes

# Show statistics
forgedb inspect stats
```

### Output Examples

**`forgedb inspect routes`**

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

**`forgedb inspect stats`**

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

## `forgedb test`

Run tests for generated code and implementations.

### Usage

```bash
forgedb test [pattern] [options]
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
forgedb test

# Run unit tests only
forgedb test --unit

# Run specific test file
forgedb test user_test

# With coverage
forgedb test --coverage
```

---

## `forgedb plugin`

Manage computed field plugins (WASM, Lua, etc.).

### Usage

```bash
forgedb plugin <command> [options]
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
forgedb plugin add ./credit_score.wasm

# List plugins
forgedb plugin list

# Remove plugin
forgedb plugin remove credit_score
```

---

## `forgedb benchmark`

Run performance benchmarks.

### Usage

```bash
forgedb benchmark [options]
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
forgedb benchmark

# Compare with SQLite
forgedb benchmark --compare sqlite

# Specific suite
forgedb benchmark --suite insert
```

### Output

```
🏃 Running Benchmarks...

Insert (1M rows):
  ForgeDB:    2.3s  ████████████████████ (baseline)
  SQLite:    3.8s  █████████████████████████████████ (+65%)

Query (scan 1M rows, filter):
  ForgeDB:    89ms  █████ (baseline)
  DuckDB:    67ms  ████ (-25%)
  SQLite:   234ms  ████████████ (+163%)

Join (10k users × 100k posts):
  ForgeDB:    42ms  ████████████████ (baseline)
  PostgreSQL: 38ms  ██████████████ (-10%)

Memory (1M rows):
  ForgeDB:    85 MB
  SQLite:   142 MB (+67%)
```

---

## Configuration File: `forgedb.toml`

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
TYPEDB_HOME          # ForgeDB installation directory
TYPEDB_CONFIG        # Path to forgedb.toml (overrides --config)
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
name: ForgeDB CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install ForgeDB CLI
        run: cargo install forgedb-cli
      
      - name: Validate Schema
        run: forgedb validate --strict
      
      - name: Generate Code
        run: forgedb generate --check
      
      - name: Run Tests
        run: forgedb test --coverage
      
      - name: Build
        run: forgedb build --release
```

---

## Troubleshooting

### Common Issues

**Schema changes not reloading:**
```bash
# Force regeneration
forgedb generate --force

# Restart dev server
forgedb dev
```

**Port already in use:**
```bash
# Use different port
forgedb dev --port 8080
```

**Generated code doesn't compile:**
```bash
# Check for schema errors
forgedb validate

# Clean and regenerate
rm -rf generated/
forgedb generate
```

**Database corruption:**
```bash
# Verify database
forgedb inspect stats

# Restore from WAL
forgedb recover

# Last resort: rebuild from migrations
forgedb migrate down --all
forgedb migrate up
```

---

## Future Commands (v2+)

```bash
forgedb export        # Export data (JSON, CSV, SQL)
forgedb import        # Import data
forgedb backup        # Create backup
forgedb restore       # Restore from backup
forgedb replicate     # Setup replication
forgedb ai            # AI assistant (v3)
```

---

**Document Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Specification
