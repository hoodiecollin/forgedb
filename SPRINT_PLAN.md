# SinkDB Sprint Plan

Incremental development plan organized into focused sprints. Each sprint builds on the previous, with Sprint 1 delivering a working end-to-end MVP.

## Current Progress

**Completed Sprints:**
- ✅ Sprint 1: MVP - End-to-End Proof of Concept
- ✅ Sprint 2: Persistence & Basic Types
- ✅ Sprint 3: Indexing & Queries
- ✅ Sprint 4: Relations (One-to-Many) + 4.1 (FK Validation & Traversal)
- ✅ Sprint 5: CLI & Developer Experience (Commands complete)
- ✅ Sprint 6: Multiple Models & Many-to-Many Relations
- ✅ Sprint 7: Write-Ahead Log & Durability
- ✅ Sprint 8: Inline Structs & Fixed Arrays
- ✅ Sprint 9: REST API Generation
- ✅ Sprint 10: TypeScript SDK Generation
- ✅ Sprint 11: Directives & Validation (Constraints)
- ✅ Sprint 12: Computed Fields
- ✅ Sprint 14: Query Optimization - Advanced Indexing (B-tree, composite indexes, range queries)

**Partially Complete:**
- None

**In Progress:**
- ⏳ Sprint 14: Query Optimization - SIMD & Query Planning (Remaining components)

**Not Started:** Sprints 13, 15-21

**Test Status:** 138/138 tests passing | 18/18 examples working

---

## Development Orchestration

This project uses git worktrees, Turborepo, and shell scripts to enable parallel development across sprints and within individual sprints.

**Full documentation**: See [ORCHESTRATOR.md](./ORCHESTRATOR.md) for complete setup and usage guide.

### Technology Stack
- **Package Manager**: `bun` (TypeScript runtime and package manager)
- **Task Runner**: Turborepo (parallel task execution with dependency management)
- **Source of Truth**: Cargo workspace structure (defines parallelizable tasks)
- **CLI**: `claude -p --permission-mode=bypassPermissions` for non-interactive parallel execution

### Quick Start Pattern
```bash
# Setup sprint (creates all orchestration assets)
bun install

# Run entire sprint in parallel
bun sprint-N

# Example for Sprint 2:
bun sprint-2
```

### Branch Naming Convention

**All work must be done in conventionally named branches**, even for sequential (non-parallel) work:

- Sprint branches: `sprint-N/main` (e.g., `sprint-1/main`, `sprint-2/main`)
- Feature branches: `sprint-N/feature-name` (e.g., `sprint-2/persistence`, `sprint-5/cli`)
- Never work directly on `main` branch

**Note**: Use `sprint-N/main` pattern (not just `sprint-N`) to allow feature branches with the same prefix.

This ensures:
- Clean git history with atomic, reviewable PRs
- Ability to parallelize work later if needed
- Consistent workflow across all sprints
- Easy rollback of individual features

### Between Sprints

Create separate worktrees for each sprint to work on multiple features simultaneously:

```bash
# From main repository
git worktree add -b sprint-2 ../kitchen-sink-sprint-2
git worktree add -b sprint-3 ../kitchen-sink-sprint-3
git worktree add -b sprint-4 ../kitchen-sink-sprint-4
```

**Benefits:**
- Work on multiple sprints in parallel without context switching
- Each sprint becomes an independent PR
- Easy to test different features in isolation
- Keep main branch stable while developing

### Within Sprints

For sprints with 3+ independent tasks, create sub-worktrees:

```bash
# From sprint-2 branch
git worktree add -b sprint-2/persistence ../kitchen-sink-sprint-2-persistence
git worktree add -b sprint-2/types ../kitchen-sink-sprint-2-types
git worktree add -b sprint-2/validation ../kitchen-sink-sprint-2-validation
```

**Recommended workflow:**
1. Create base sprint branch (e.g., `sprint-2`)
2. Create feature branches for independent tasks
3. Submit separate PRs for each feature into the sprint branch
4. Merge sprint branch to main when complete

**Example PR structure:**
```
main
└── sprint-2
    ├── sprint-2/persistence → PR #1 (into sprint-2)
    ├── sprint-2/types → PR #2 (into sprint-2)
    ├── sprint-2/validation → PR #3 (into sprint-2)
    └── sprint-2/tests → PR #4 (into sprint-2, after #1-3 merge)

    Then: sprint-2 → PR #5 (into main)
```

### Worktree Management

**List all worktrees:**
```bash
git worktree list
```

**Remove a worktree:**
```bash
git worktree remove ../kitchen-sink-sprint-2-persistence
```

**Prune deleted worktrees:**
```bash
git worktree prune
```

### When to Use Worktrees

**Good candidates for parallel worktrees:**
- Sprint 2: Storage, Types, Validation (all independent)
- Sprint 3: Indexing and Query Operations
- Sprint 5: CLI commands, Project structure, File watching
- Sprint 9: Different CRUD endpoint implementations

**Keep serialized:**
- Sprint 1: Small MVP, needs full integration
- Sprint 7: WAL components are tightly coupled
- Sprint 16: Migrations are sequential by nature

### Orchestrator Implementation

The orchestrator automates parallel development by:
1. Parsing sprint metadata from this document
2. Setting up Cargo workspace members as git worktrees
3. Generating Turborepo configuration aligned with Cargo.toml
4. Running parallel `claude -p` instances for each task

**Source of truth**: `Cargo.toml` workspace members
**Alignment**: npm/bun workspaces mirror Cargo workspace structure

**Example orchestrator invocation:**
```bash
# Run orchestrator for Sprint 2
bun run orchestrate sprint-2

# Generated structure:
# .orchestrator/
# ├── package.json          # bun workspace root
# ├── turbo.json            # Turborepo pipeline config
# └── worktrees/
#     ├── persistence/      # mirrors Cargo member: storage
#     ├── types/            # mirrors Cargo member: types
#     └── validation/       # mirrors Cargo member: validation
```

**Orchestrator workflow:**
1. Parse sprint YAML metadata
2. Read `Cargo.toml` workspace members
3. Create git worktree for each Cargo member
4. Generate bun workspace with matching structure
5. Create `turbo.json` with dependency graph
6. Generate package.json scripts: `claude -p --permission-mode=bypassPermissions "{prompt}"`
7. Run: `turbo run build --parallel`
8. Collect logs from `.turbo/runs/`

**Permission mode**: `bypassPermissions` ensures Claude workers can create files, run commands, and make changes without interactive prompts.

**Key insight**: Cargo workspace members define the code structure, orchestrator just automates parallel Claude invocations for each member.

---

## Sprint 1: MVP - End-to-End Proof of Concept ✅ COMPLETE

**Goal**: Demonstrate the core concept with a minimal but complete working system.

**Deliverables**: A working schema → code → database → query pipeline for a single simple model.

**Status**: ✅ Completed

**Orchestration**: ⚠️ Serialized (small MVP, needs full integration)
```yaml
parallelization: none
reason: "MVP is small and tightly integrated - better as single implementation"
```

### Tasks

#### Schema Parser (Minimal)
- [ ] Lexer for basic tokens (identifiers, types, symbols)
- [ ] Parser for single model declaration
- [ ] Support primitive types only: `u32`, `u64`, `string`
- [ ] Support basic symbols: `+` (auto-generate), `&` (unique)
- [ ] AST representation for single model
- [ ] Basic error reporting

**Test Schema:**
```
User {
  id: +u64
  email: &string
}
```

#### Code Generator (Minimal)
- [ ] AST → Rust struct generation
- [ ] Generate basic storage struct (in-memory only)
- [ ] Generate insert function
- [ ] Generate get-by-id function
- [ ] No optimization, just working code

**Generated Output:**
- Single Rust file with User struct and basic operations

#### Storage Engine (In-Memory)
- [ ] In-memory Vec-based storage (no persistence)
- [ ] Auto-increment for `+u64`
- [ ] Unique constraint check for `&string`
- [ ] Insert operation
- [ ] Get operation
- [ ] Tombstone bitmap for deletes

#### Integration
- [ ] CLI stub that parses schema file
- [ ] Generates Rust code to `./generated/`
- [ ] Example main.rs that uses generated code
- [ ] Compiles and runs successfully

#### Success Criteria
- [x] Parse simple schema
- [x] Generate compilable Rust code
- [x] Insert users with auto-increment ID
- [x] Enforce unique email constraint
- [x] Retrieve users by ID
- [x] All in-memory, no crashes

**Output**: `cargo run` executes schema → codegen → database → query

---

## Sprint 2: Persistence & Basic Types ✅ COMPLETE

**Goal**: Add persistence and expand type support.

**Status**: ✅ Completed

**Orchestration**: ✅ Parallelizable
```yaml
cargo_workspace_members:
  - storage     # Storage persistence implementation
  - types       # Type system and primitives
  - validation  # Schema validation
  - tests       # Integration tests (depends on all)

tasks:
  - name: persistence
    cargo_member: storage
    branch: sprint-2/persistence
    dependencies: []
    prompt: |
      Implement storage persistence for Sprint 2:
      - Memory-mapped files for fixed-size columns (write u64 to fixed/u64_0.bin)
      - Variable-length string storage (variable/string_data.bin + offsets)
      - Basic manifest.json for metadata
      - Ensure database survives restart
      - Write comprehensive tests

  - name: types
    cargo_member: types
    branch: sprint-2/types
    dependencies: []
    prompt: |
      Implement expanded type support for Sprint 2:
      - Add primitive types: i32, i64, f64, bool
      - Add uuid type with auto-generation
      - Add timestamp type with auto-set (+timestamp)
      - Update parser to handle all primitive types
      - Update codegen for each type
      - Write unit tests for each type

  - name: validation
    cargo_member: validation
    branch: sprint-2/validation
    dependencies: []
    prompt: |
      Implement schema validation for Sprint 2:
      - Validate field names (snake_case enforcement)
      - Validate model names (PascalCase enforcement)
      - Check for duplicate field names
      - Better error messages with line numbers and suggestions
      - Write validation unit tests

  - name: integration-tests
    cargo_member: tests
    branch: sprint-2/tests
    dependencies: [persistence, types, validation]
    prompt: |
      Write comprehensive integration tests for Sprint 2:
      - Unit tests for parser with all types
      - Unit tests for each storage type
      - Persistence test (write → close → reopen → read)
      - Test schema with all primitive types
      - Verify all success criteria
```

### Tasks

#### Storage Persistence
- [ ] Memory-mapped files for fixed-size columns
- [ ] Write u64 column to `fixed/u64_0.bin`
- [ ] Write string to `variable/string_data.bin` + offsets
- [ ] Basic manifest.json for metadata
- [ ] Database survives restart

#### Expanded Type Support
- [ ] Add: `i32`, `i64`, `f64`, `bool`
- [ ] Add: `uuid` type with auto-generation
- [ ] Add: `timestamp` type with auto-set (`+timestamp`)
- [ ] Update parser to handle all primitive types
- [ ] Update codegen for each type

#### Schema Validation
- [ ] Validate field names (snake_case)
- [ ] Validate model names (PascalCase)
- [ ] Check for duplicate field names
- [ ] Better error messages with line numbers

#### Tests
- [ ] Unit tests for parser
- [ ] Unit tests for each storage type
- [ ] Test persistence (write → close → reopen → read)

**Test Schema:**
```
User {
  id: +uuid
  email: &string
  age: u32
  balance: f64
  active: bool
  created_at: +timestamp
}
```

#### Success Criteria
- [x] Support all primitive types
- [x] Data persists across restarts
- [x] Memory-mapped file I/O works
- [x] Tests pass

---

## Sprint 3: Indexing & Queries ✅ COMPLETE

**Goal**: Add indexes and basic query capabilities.

**Status**: ✅ Completed

**Orchestration**: ✅ Parallelizable
```yaml
cargo_workspace_members:
  - indexing    # Index data structures and operations
  - queries     # Query operations and filtering
  - codegen     # Code generation for query methods

tasks:
  - name: indexing
    cargo_member: indexing
    branch: sprint-3/indexing
    dependencies: []
    prompt: |
      Implement indexing for Sprint 3:
      - Add index symbol '^' support in parser
      - Hash index for '^&' fields (indexed + unique)
      - Hash index for '^' fields (indexed only)
      - Index stored in-memory (rebuild on load)
      - Write index persistence and rebuild tests

  - name: queries
    cargo_member: queries
    branch: sprint-3/queries
    dependencies: []
    prompt: |
      Implement query operations for Sprint 3:
      - List all with tombstone filtering
      - Filter by indexed field (O(1) lookup)
      - Filter by equality on any field (O(n) scan)
      - Update operation with index maintenance
      - Delete operation with tombstone marking
      - Write comprehensive query tests

  - name: codegen
    cargo_member: codegen
    branch: sprint-3/codegen
    dependencies: [indexing, queries]
    prompt: |
      Generate query methods for Sprint 3:
      - Generate find_by_X for '^' indexed fields
      - Generate list() function with filtering
      - Generate update() function
      - Generate delete() function
      - Write codegen tests and verify generated code compiles
```

### Tasks

#### Indexing
- [ ] Index symbol `^` support in parser
- [ ] Hash index for `^&` fields (indexed + unique)
- [ ] Hash index for `^` fields (indexed only)
- [ ] Index stored in-memory (rebuild on load)

#### Query Operations
- [ ] List all (with tombstone filtering)
- [ ] Filter by indexed field (O(1) lookup)
- [ ] Filter by equality on any field (O(n) scan)
- [ ] Update operation
- [ ] Delete operation (tombstone marking)

#### Code Generation
- [ ] Generate `find_by_X` for `^` fields
- [ ] Generate `list()` function
- [ ] Generate `update()` function
- [ ] Generate `delete()` function

**Test Schema:**
```
User {
  id: +uuid
  email: ^&string
  username: ^string
  age: u32
}
```

#### Success Criteria
- [x] Fast lookup by indexed fields
- [x] CRUD operations complete
- [x] Indexes rebuilt on database load
- [x] Tombstones prevent deleted records from appearing

---

## Sprint 4: Relations (One-to-Many) ✅ COMPLETE

**Goal**: Support basic relationships.

**Status**: ✅ Completed (including Sprint 4.1 FK validation and traversal)

**Orchestration**: ⚠️ Partially Serialized
```yaml
cargo_workspace_members:
  - relations   # Relation parsing and FK generation
  - storage     # FK storage and validation (extends Sprint 2)
  - codegen     # Relation method generation (extends Sprint 3)

tasks:
  - name: schema-relations
    cargo_member: relations
    branch: sprint-4/schema
    dependencies: []
    prompt: |
      Implement relation parsing for Sprint 4:
      - Parse relation syntax: posts: [Post] and author: *User
      - Detect one-to-many patterns
      - Generate foreign key column definitions
      - Build relation graph and detect cycles

  - name: storage-fk
    cargo_member: storage
    branch: sprint-4/storage
    dependencies: [schema-relations]
    prompt: |
      Implement FK storage for Sprint 4:
      - Store foreign key as regular indexed column
      - Index foreign keys automatically
      - Validate foreign key references exist
      - Write FK constraint tests

  - name: codegen-relations
    cargo_member: codegen
    branch: sprint-4/codegen
    dependencies: [schema-relations, storage-fk]
    prompt: |
      Generate relation methods for Sprint 4:
      - Generate FK column in child model
      - Generate relation traversal: user.posts()
      - Generate reverse lookup: post.author()
      - Write integration tests with test schema
```

### Tasks

#### Schema Support
- [ ] Parse relation syntax: `posts: [Post]`
- [ ] Parse required relation: `author: *User`
- [ ] Detect one-to-many patterns
- [ ] Generate foreign key columns

#### Storage
- [ ] Store foreign key as regular column
- [ ] Index foreign keys automatically
- [ ] Validate foreign key references exist

#### Code Generation
- [ ] Generate FK column in child model
- [ ] Generate relation traversal function: `user.posts()`
- [ ] Generate reverse lookup: `post.author()`

**Test Schema:**
```
User {
  id: +uuid
  email: &string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}
```

#### Success Criteria
- [x] Can create posts linked to users
- [x] Can traverse user → posts
- [x] Can traverse post → user
- [x] Foreign key validation works

---

## Sprint 5: CLI & Developer Experience ✅ PARTIALLY COMPLETE

**Goal**: Usable CLI tool with good DX.

**Status**: ✅ CLI commands implemented (cli crate complete)

**Orchestration**: ✅ Highly Parallelizable
```yaml
cargo_workspace_members:
  - cli         # CLI framework and commands
  - scaffold    # Project scaffolding
  - watcher     # File watching and auto-regen
  - docs        # Documentation generation

tasks:
  - name: cli-commands
    cargo_member: cli
    branch: sprint-5/cli
    dependencies: []
    prompt: |
      Implement CLI commands for Sprint 5:
      - sinkdb init <project> - scaffold new project
      - sinkdb generate - generate code from schema
      - sinkdb validate - validate schema
      - sinkdb build - compile generated code
      - CLI help text and error messages with suggestions

  - name: scaffolding
    cargo_member: scaffold
    branch: sprint-5/scaffold
    dependencies: []
    prompt: |
      Implement project scaffolding for Sprint 5:
      - Generate standard project layout
      - Create schema.lang template file
      - Create sinkdb.toml config
      - Generate .gitignore with Rust/DB entries
      - Write scaffolding tests

  - name: file-watcher
    cargo_member: watcher
    branch: sprint-5/watcher
    dependencies: []
    prompt: |
      Implement file watching for Sprint 5:
      - Watch schema.lang for changes using notify crate
      - Auto-regenerate on schema change
      - Clear error display in terminal
      - Debounce rapid changes
      - Write watcher integration tests

  - name: documentation
    cargo_member: docs
    branch: sprint-5/docs
    dependencies: [cli-commands, scaffolding]
    prompt: |
      Create documentation for Sprint 5:
      - Getting started guide (installation, first project)
      - CLI help text for all commands
      - Error messages with actionable suggestions
      - Example project walkthrough
```

### Tasks

#### CLI Commands (✅ COMPLETE - crates/cli)
- [x] `sinkdb init <project>` - scaffolds new project
- [x] `sinkdb generate` - generates code from schema
- [x] `sinkdb validate` - validates schema
- [x] `sinkdb build` - compiles generated code

#### Project Structure (✅ COMPLETE)
- [x] Generate standard project layout
- [x] Create `schema.sink` file (with templates: blank, blog, ecommerce, todo)
- [x] Create `sinkdb.toml` config
- [x] Generate `.gitignore`
- [x] Generate README.md
- [x] Generate Cargo.toml and Rust project files

#### File Watching (✅ COMPLETE)
- [x] Watch `schema.sink` for changes
- [x] Auto-regenerate on schema change
- [x] Clear terminal display
- [x] Debouncing support (configurable)
- [x] `sinkdb dev` command with options

#### Documentation (✅ COMPLETE)
- [x] CLI help text for all commands
- [x] Error messages with suggestions
- [x] Getting started guide (SPRINT5_CLI.md)
- [x] Example demo script (examples/cli_demo.sh)

**Success Criteria:** ✅ All Met
```bash
$ sinkdb init my-app
$ cd my-app
$ sinkdb dev  # watches and regenerates automatically

# Options:
$ sinkdb dev --schema schema.sink --output generated
$ sinkdb dev --debounce 500  # custom debounce delay
$ sinkdb dev --clear=false   # disable terminal clearing
```

**Features:**
- ✅ Auto-regeneration on schema changes
- ✅ Debouncing (default 200ms, configurable)
- ✅ Clear terminal output (optional)
- ✅ Colored console output with status
- ✅ Initial generation on startup
- ✅ Error handling and display

---

## Sprint 6: Multiple Models & Relations ✅ COMPLETE

**Goal**: Support complex multi-model schemas.

**Status**: ✅ Completed

### Tasks

#### Parser Enhancements
- [ ] Parse multiple model declarations
- [ ] Parse many-to-many relations
- [ ] Detect bidirectional relations
- [ ] Build symbol table for cross-references

#### Junction Tables
- [ ] Generate junction table for many-to-many
- [ ] Add/remove operations for M:N
- [ ] Query operations for M:N

#### Code Organization
- [ ] Generate separate file per model
- [ ] Generate mod.rs with exports
- [ ] Database struct manages all models

**Test Schema:**
```
User {
  id: +uuid
  posts: [Post]
  liked_posts: [Post]
}

Post {
  id: +uuid
  author: *User
  tags: [Tag]
  liked_by: [User]
}

Tag {
  id: +uuid
  posts: [Post]
}
```

#### Success Criteria
- [x] Multiple models in one schema
- [x] Many-to-many relations work
- [x] Junction tables auto-generated
- [x] Clean code organization

---

## Sprint 7: Write-Ahead Log & Durability ✅ COMPLETE

**Goal**: ACID properties and crash recovery.

**Status**: ✅ Completed

**Orchestration**: ⚠️ Serialized (tightly coupled components)
```yaml
cargo_workspace_members:
  - wal         # WAL implementation (all components tightly integrated)

reason: |
  WAL components are tightly coupled - file format, recovery, and transactions
  all depend on each other. Better implemented as single cohesive unit.

prompt: |
  Implement Write-Ahead Log for Sprint 7:
  - Design WAL file format (operation type + data)
  - Write operations to WAL before data files
  - Configurable fsync policy (immediate, periodic, none)
  - WAL replay on startup with incomplete entry detection
  - Truncate corrupted WAL entries
  - WAL rotation and cleanup
  - Begin/commit/rollback transaction API
  - Group operations in transaction with atomic commit
  - Comprehensive tests: crash recovery, commit, rollback, corruption
  - Performance: recovery < 1s for 10k writes
```

### Tasks

#### WAL Implementation
- [ ] WAL file format design
- [ ] Write operations to WAL before data files
- [ ] WAL entry: operation type + data
- [ ] Configurable fsync policy

#### Recovery
- [ ] Replay WAL on startup
- [ ] Detect incomplete entries
- [ ] Truncate corrupted WAL
- [ ] WAL rotation/cleanup

#### Transactions (Basic)
- [ ] Begin/commit/rollback API
- [ ] Group operations in transaction
- [ ] Atomic commit via WAL

#### Tests
- [ ] Insert → crash → recover → verify
- [ ] Transaction commit test
- [ ] Transaction rollback test
- [ ] WAL corruption handling

#### Success Criteria
- [x] No data loss on crash
- [x] Transactions are atomic
- [x] WAL replays correctly
- [x] Recovery < 1s for 10k writes

---

## Sprint 8: Inline Structs & Fixed Arrays ✅ COMPLETE

**Goal**: Support compound fixed-size types.

**Status**: ✅ Completed

### Tasks

#### Parser
- [ ] Parse `struct` declarations
- [ ] Parse inline struct fields: `address: Address`
- [ ] Parse fixed arrays: `[f64; 3]`
- [ ] Validate: no variable-length in structs

#### Storage
- [ ] Calculate struct size and alignment
- [ ] Store structs as single fixed-size block
- [ ] Handle nested structs
- [ ] Zero-copy field access

#### Code Generation
- [ ] Generate Rust struct definitions
- [ ] Generate field accessors
- [ ] Maintain alignment and padding

**Test Schema:**
```
struct Address {
  street: char(100)
  city: char(50)
  zip: char(10)
}

struct Location {
  lat: f64
  lon: f64
}

User {
  id: +uuid
  address: Address
  location: Location?
  tags: [char(20); 5]
}
```

#### Success Criteria
- [x] Inline structs stored efficiently
- [x] Nested structs work
- [x] Fixed arrays work
- [x] Zero-copy access

---

## Sprint 9: REST API Generation ✅ COMPLETE

**Goal**: Auto-generate REST API from schema.

**Status**: ✅ Completed

**Orchestration**: ✅ Highly Parallelizable
```yaml
cargo_workspace_members:
  - http-server  # HTTP server setup (Axum/Actix)
  - crud-api     # CRUD endpoint handlers
  - query-params # Query parameter parsing and filtering
  - validation   # Request validation and error handling
  - codegen      # API code generation

tasks:
  - name: http-server
    cargo_member: http-server
    branch: sprint-9/server
    dependencies: []
    prompt: |
      Implement HTTP server for Sprint 9:
      - Choose framework (Axum recommended)
      - Basic server setup with routing
      - JSON serialization/deserialization
      - Error response formatting
      - Server integration tests

  - name: crud-endpoints
    cargo_member: crud-api
    branch: sprint-9/crud
    dependencies: []
    prompt: |
      Implement CRUD endpoints for Sprint 9:
      - GET /api/{model} - list
      - GET /api/{model}/{id} - get by ID
      - POST /api/{model} - create
      - PUT /api/{model}/{id} - update
      - DELETE /api/{model}/{id} - delete
      - Write endpoint tests

  - name: query-params
    cargo_member: query-params
    branch: sprint-9/query
    dependencies: []
    prompt: |
      Implement query parameters for Sprint 9:
      - Filter: ?email=x@y.com (equality matching)
      - Sort: ?sort=created_at&order=desc
      - Pagination: ?limit=50&offset=100
      - Query parser with validation
      - Write query parsing tests

  - name: validation
    cargo_member: validation
    branch: sprint-9/validation
    dependencies: []
    prompt: |
      Implement request validation for Sprint 9:
      - Validate request body against schema
      - Return 400 for validation errors with details
      - Return 409 for unique constraint violations
      - Return 404 for not found
      - Write validation tests

  - name: codegen-api
    cargo_member: codegen
    branch: sprint-9/codegen
    dependencies: [http-server, crud-endpoints, query-params, validation]
    prompt: |
      Generate API code for Sprint 9:
      - Generate handler functions for each model
      - Generate request/response types (serde)
      - Generate validation logic
      - Generate router setup
      - Test generated API with curl
```

### Tasks

#### HTTP Server
- [x] Choose framework (Axum or Actix-web)
- [x] Basic server setup
- [x] Route generation from schema

#### CRUD Endpoints
- [x] `GET /api/users` - list with query params
- [x] `GET /api/users/{id}` - get by ID
- [x] `POST /api/users` - create
- [x] `PUT /api/users/{id}` - update
- [x] `DELETE /api/users/{id}` - delete

#### Query Parameters
- [x] Filter: `?email=x@y.com`
- [x] Sort: `?sort=created_at&order=desc`
- [x] Pagination: `?limit=50&offset=100`

#### Validation
- [x] Validate request body against schema
- [x] Return 400 for validation errors
- [x] Return 409 for unique violations

#### Code Generation
- [x] Generate handler functions
- [x] Generate request/response types
- [x] Generate validation logic

#### Success Criteria
- [x] All CRUD endpoints working
- [x] Query parameters implemented
- [x] Validation and error handling complete
- [x] Generated code compiles and runs successfully

---

## Sprint 10: TypeScript SDK Generation ✅ COMPLETE

**Goal**: Type-safe client for generated APIs.

**Status**: ✅ Completed

### Tasks

#### Type Generation
- [x] Generate TypeScript interfaces from schema
- [x] Generate API client class
- [x] Generate SDK types for relations

#### API Client
- [x] `UserApi.list(params)`
- [x] `UserApi.get(id)`
- [x] `UserApi.create(data)`
- [x] `UserApi.update(id, data)`
- [x] `UserApi.delete(id)`

#### Relations
- [x] `UserApi.posts(userId)` - traverse relations
- [x] Type-safe relation parameters

#### NPM Package
- [x] Generate package.json
- [x] Bundle with tsup or rollup
- [x] Publish-ready structure

**Generated Output:**
```typescript
import { SinkDBClient } from '@sinkdb/client'

const client = new SinkDBClient('http://localhost:3000')
const users = await client.user.list({ email: 'test@example.com' })
const user = await client.user.get(users.data[0].id)
```

#### Success Criteria
- [x] TypeScript types match schema exactly
- [x] API client is type-safe
- [x] Complete NPM package with bundling
- [x] Works with popular frameworks (React, Vue)

---

## Sprint 11: Directives & Validation ✅ COMPLETE

**Goal**: Schema directives for validation and behavior.

**Status**: ✅ Completed

### Tasks

#### Parser Support
- [x] Parse `@email`, `@url`, `@phone`
- [x] Parse `@min`, `@max`, `@pattern`
- [x] Parse constraint parameters (number and string)
- [x] Support multiple constraints per field

#### Validation Logic
- [x] Email format validation (regex-based)
- [x] URL format validation (regex-based)
- [x] Pattern matching (regex)
- [x] Min/max for numbers (boundary checking)
- [x] String length constraints

#### Code Generation
- [x] Generate validation functions
- [x] Generate constraint checking in insert method
- [x] Error messages for validation failures
- [x] Regex import when needed

#### Helper Methods
- [x] `has_constraint()` - Check if field has specific constraint
- [x] `get_constraint()` - Retrieve constraint by name
- [x] `is_nullable()` - Check if field is nullable

**Test Schema:**
```
User {
  id: +uuid
  email: ^&string @email
  website: string? @url
  age: u32 @min(0) @max(150)
  password: string @private
}
```

#### Success Criteria
- [x] Validation runs on insert/update
- [x] Email/URL validation working
- [x] Min/max constraints enforced
- [x] Descriptive error messages generated
- [x] Multiple constraints per field supported

---

## Sprint 12: Computed Fields ✅ COMPLETE

**Goal**: Support computed/derived fields.

**Status**: ✅ Completed

### Tasks

#### Parser
- [x] Parse `@computed` directive
- [x] Identify computed field dependencies

#### Code Generation
- [x] Generate trait for computed fields
- [x] Generate stub implementation
- [x] Client-side computation by default

#### Runtime
- [x] Compute on access (lazy)
- [x] Generate accessor methods
- [x] Include in API responses

**Test Schema:**
```
User {
  first_name: string
  last_name: string
  full_name: string @computed

  posts: [Post]
  post_count: u32 @computed
}
```

**Generated Trait:**
```rust
trait UserComputed {
  fn full_name(instance: &User) -> String;
  fn post_count(instance: &User) -> u32;
}
```

**Generated Accessor Methods:**
```rust
impl UserStorage {
  pub fn compute_full_name<C: UserComputed>(&self, id: Uuid) -> Option<String>;
  pub fn compute_post_count<C: UserComputed>(&self, id: Uuid) -> Option<u32>;
}
```

#### Success Criteria
- [x] Computed fields work client-side ✅
- [x] Trait system allows customization ✅
- [x] API includes computed fields in responses ✅
- [x] TypeScript SDK includes computed fields ✅
- [x] Excluded from Create/Update requests ✅
- [x] Comprehensive tests (8 tests) ✅

---

## Sprint 13: OpenAPI & Documentation

**Goal**: Complete API documentation.

### Tasks

#### OpenAPI Generation
- [ ] Generate OpenAPI 3.0 spec from schema
- [ ] Include all endpoints
- [ ] Include request/response schemas
- [ ] Include query parameters

#### API Documentation UI
- [ ] Serve Swagger UI at `/docs`
- [ ] Interactive API testing
- [ ] Example requests/responses

#### Schema Documentation
- [ ] Generate markdown docs from schema
- [ ] Document models, fields, relations
- [ ] Document computed fields
- [ ] Document directives

#### Success Criteria
- [x] Complete OpenAPI spec validates
- [x] Swagger UI works
- [x] All endpoints documented
- [x] Schema docs auto-generated

---

## Sprint 14: Query Optimization ✅ PARTIALLY COMPLETE

**Goal**: Optimize query performance.

**Status**: ✅ Advanced indexing complete; SIMD/query planning pending

### Tasks

#### Columnar Scanning
- [ ] SIMD operations for numeric filters
- [ ] Batch processing (1024 rows at a time)
- [ ] Early termination for limits

#### Index Improvements
- [x] B-tree index for range queries (ordered types: numeric, timestamp)
- [x] Hash index for unordered types (string, bool, uuid)
- [x] Composite indexes with @index(field1, field2, ...) directive
- [x] Automatic index type selection based on field type
- [x] Range query methods (_range, _gt, _gte, _lt, _lte)
- [x] ordered-float integration for f64 in BTreeMap
- [x] Index maintenance on insert/update/delete
- [ ] Index statistics

#### Query Planning
- [ ] Cost-based index selection
- [ ] Join order optimization
- [ ] Predicate pushdown

#### Benchmarks
- [ ] Scan 1M rows: < 100ms
- [ ] Index lookup: < 1μs
- [ ] Join 10k → 100k: < 50ms

#### Success Criteria
- [x] B-tree indexes for range queries implemented
- [x] Composite indexes working
- [x] Automatic index type selection
- [x] Range query methods generated
- [ ] SIMD operations for numeric filters (pending)
- [ ] Cost-based query planning (pending)

---

## Sprint 15: Compaction & Maintenance

**Goal**: Background maintenance and optimization.

### Tasks

#### Compaction
- [ ] Identify dead space in variable columns
- [ ] Compact on threshold (e.g., 30% dead)
- [ ] Background compaction thread
- [ ] Non-blocking compaction

#### Statistics
- [ ] Track row count, disk usage
- [ ] Track index sizes
- [ ] Query performance metrics

#### Maintenance API
- [ ] Manual compaction trigger
- [ ] Vacuum command
- [ ] Analyze/optimize command

#### Success Criteria
- [x] Compaction reclaims 90%+ dead space
- [x] Background compaction doesn't block queries
- [x] Statistics accurate

---

## Sprint 16: Migrations

**Goal**: Schema evolution and migrations.

### Tasks

#### Schema Diffing
- [ ] Compare old schema → new schema
- [ ] Detect: added fields, removed fields, type changes
- [ ] Categorize: safe vs breaking changes

#### Migration Generation
- [ ] Generate migration files
- [ ] Add column migrations
- [ ] Remove column migrations (with warning)
- [ ] Type change migrations (manual required)

#### Migration Execution
- [ ] `sinkdb migrate up`
- [ ] `sinkdb migrate down` (rollback)
- [ ] Track applied migrations
- [ ] Prevent partial migrations

#### CLI
- [ ] `sinkdb migrate create "description"`
- [ ] `sinkdb migrate status`
- [ ] `sinkdb migrate up`
- [ ] `sinkdb migrate down`

#### Success Criteria
- [x] Safe migrations automatic
- [x] Breaking changes warned
- [x] Rollback works
- [x] No data loss

---

## Sprint 17: UI Component Integration

**Goal**: Connect schemas to UI components.

### Tasks

#### Schema Syntax
- [ ] Parse component references: `jsx://path/to/Component.jsx`
- [ ] Support multiple formats (jsx, svelte, vue)
- [ ] Validate component paths exist

#### Props Generation
- [ ] Generate TypeScript props types
- [ ] Include model data
- [ ] Include computed fields
- [ ] Include relations (optional)

#### Scaffolding
- [ ] Generate component stubs
- [ ] Hot reload on schema changes
- [ ] Type-safe props

**Test Schema:**
```
User {
  id: +uuid
  email: string

  card: jsx://components/UserCard.tsx
  profile: jsx://views/UserProfile.tsx
}
```

**Generated Props:**
```typescript
type UserCardProps = {
  data: User
  computed?: UserComputed
  relations?: {
    posts?: Post[]
  }
}
```

#### Success Criteria
- [x] Component stubs generated
- [x] Props are type-safe
- [x] Works with React/Svelte/Vue

---

## Sprint 18: Full-Text Search

**Goal**: Advanced text search capabilities.

### Tasks

#### Indexing
- [ ] Parse `@fulltext` directive
- [ ] Build trigram or inverted index
- [ ] Update index on insert/update/delete

#### Query Support
- [ ] Full-text search syntax: `?q=search terms`
- [ ] Ranking/relevance scoring
- [ ] Phrase search
- [ ] Boolean operators (AND, OR, NOT)

#### API
- [ ] `GET /api/posts?q=search+terms`
- [ ] Pagination for search results
- [ ] Highlighting (optional)

#### Success Criteria
- [x] Fast full-text search
- [x] Relevance ranking works
- [x] Search via API

---

## Sprint 19: Advanced Features (Select)

**Goal**: High-value advanced features.

### Tasks

#### Materialized Computed Fields
- [ ] `@materialized` directive
- [ ] Store computed result
- [ ] Invalidate on dependency change

#### Partial Field Selection
- [ ] API: `?fields=id,email,created_at`
- [ ] Only load selected columns
- [ ] Reduce response size

#### Batch Operations
- [ ] Batch create: `POST /api/users/batch`
- [ ] Batch update
- [ ] Batch delete
- [ ] Transactional batches

#### Soft Delete
- [ ] `@soft_delete` directive
- [ ] Add `deleted_at` timestamp
- [ ] Filter deleted by default
- [ ] `?include_deleted=true` to show

#### Success Criteria
- [x] Materialized fields cached correctly
- [x] Field selection reduces payload size
- [x] Batch operations work
- [x] Soft delete implemented

---

## Sprint 20: Production Readiness

**Goal**: Production-grade reliability and observability.

### Tasks

#### Error Handling
- [ ] Comprehensive error types
- [ ] Error codes for API
- [ ] Helpful error messages
- [ ] Error logging

#### Observability
- [ ] Structured logging
- [ ] Metrics (Prometheus format)
- [ ] Health check endpoint
- [ ] Trace IDs

#### Performance
- [ ] Connection pooling
- [ ] Response caching
- [ ] Rate limiting
- [ ] CORS configuration

#### Security
- [ ] Input sanitization
- [ ] SQL injection prevention (N/A but validate)
- [ ] Auth hooks
- [ ] TLS support

#### Documentation
- [ ] Deployment guide
- [ ] Configuration reference
- [ ] Troubleshooting guide
- [ ] Best practices

#### Success Criteria
- [x] Production deployment succeeds
- [x] Monitoring in place
- [x] Security hardened
- [x] Complete documentation

---

## Sprint 21+: Future Enhancements

### Potential Features (Prioritize Later)

#### WASM Support
- Compile database to WASM
- Browser-based storage (IndexedDB)
- Local-first applications

#### Distributed/Replication
- Multi-node support
- Leader election
- Replication protocol

#### Advanced Indexing
- Spatial indexes (R-tree)
- Vector embeddings (HNSW)
- Composite indexes

#### MVCC Concurrency
- Multi-version concurrency control
- Snapshot isolation
- Better write concurrency

#### AI-Powered Development
- `@ai-implement` annotations
- Auto-generate component code
- Test generation

---

## Sprint Success Metrics

Each sprint should achieve:
- ✅ All tasks completed
- ✅ Tests passing (unit + integration)
- ✅ Documentation updated
- ✅ Example working
- ✅ No regressions

## Definition of Done

A sprint is complete when:
1. All features implemented and tested
2. Code is reviewed and merged
3. Documentation is updated
4. Examples demonstrate the feature
5. Performance targets met (if applicable)
6. No known critical bugs

---

**Document Version**: 1.0
**Last Updated**: October 11, 2025
**Status**: Active Planning
