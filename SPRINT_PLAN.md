# SinkDB Sprint Plan

Incremental development plan organized into focused sprints. Each sprint builds on the previous, with Sprint 1 delivering a working end-to-end MVP.

---

## Sprint 1: MVP - End-to-End Proof of Concept

**Goal**: Demonstrate the core concept with a minimal but complete working system.

**Deliverables**: A working schema → code → database → query pipeline for a single simple model.

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

## Sprint 2: Persistence & Basic Types

**Goal**: Add persistence and expand type support.

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

## Sprint 3: Indexing & Queries

**Goal**: Add indexes and basic query capabilities.

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

## Sprint 4: Relations (One-to-Many)

**Goal**: Support basic relationships.

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

## Sprint 5: CLI & Developer Experience

**Goal**: Usable CLI tool with good DX.

### Tasks

#### CLI Commands
- [ ] `sinkdb init <project>` - scaffolds new project
- [ ] `sinkdb generate` - generates code from schema
- [ ] `sinkdb validate` - validates schema
- [ ] `sinkdb build` - compiles generated code

#### Project Structure
- [ ] Generate standard project layout
- [ ] Create `schema.lang` file
- [ ] Create `sinkdb.toml` config
- [ ] Generate `.gitignore`

#### File Watching
- [ ] Watch `schema.lang` for changes
- [ ] Auto-regenerate on schema change
- [ ] Clear error display in terminal

#### Documentation
- [ ] CLI help text
- [ ] Error messages with suggestions
- [ ] Getting started guide

**Success Criteria:**
```bash
$ sinkdb init my-app
$ cd my-app
$ sinkdb dev  # watches and regenerates
```

---

## Sprint 6: Multiple Models & Relations

**Goal**: Support complex multi-model schemas.

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

## Sprint 7: Write-Ahead Log & Durability

**Goal**: ACID properties and crash recovery.

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

## Sprint 8: Inline Structs & Fixed Arrays

**Goal**: Support compound fixed-size types.

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

## Sprint 9: REST API Generation

**Goal**: Auto-generate REST API from schema.

### Tasks

#### HTTP Server
- [ ] Choose framework (Axum or Actix-web)
- [ ] Basic server setup
- [ ] Route generation from schema

#### CRUD Endpoints
- [ ] `GET /api/users` - list with query params
- [ ] `GET /api/users/{id}` - get by ID
- [ ] `POST /api/users` - create
- [ ] `PUT /api/users/{id}` - update
- [ ] `DELETE /api/users/{id}` - delete

#### Query Parameters
- [ ] Filter: `?email=x@y.com`
- [ ] Sort: `?sort=created_at&order=desc`
- [ ] Pagination: `?limit=50&offset=100`

#### Validation
- [ ] Validate request body against schema
- [ ] Return 400 for validation errors
- [ ] Return 409 for unique violations

#### Code Generation
- [ ] Generate handler functions
- [ ] Generate request/response types
- [ ] Generate validation logic

#### Success Criteria
```bash
$ curl http://localhost:3000/api/users
$ curl -X POST http://localhost:3000/api/users \
  -d '{"email":"test@example.com"}'
```

---

## Sprint 10: TypeScript SDK Generation

**Goal**: Type-safe client for generated APIs.

### Tasks

#### Type Generation
- [ ] Generate TypeScript interfaces from schema
- [ ] Generate API client class
- [ ] Generate SDK types for relations

#### API Client
- [ ] `UserApi.list(params)`
- [ ] `UserApi.get(id)`
- [ ] `UserApi.create(data)`
- [ ] `UserApi.update(id, data)`
- [ ] `UserApi.delete(id)`

#### Relations
- [ ] `UserApi.posts(userId)` - traverse relations
- [ ] Type-safe relation parameters

#### NPM Package
- [ ] Generate package.json
- [ ] Bundle with tsup or rollup
- [ ] Publish-ready structure

**Generated Output:**
```typescript
import { UserApi, User } from './generated'

const api = new UserApi('http://localhost:3000')
const users = await api.list({ email: 'test@example.com' })
const user = await api.get(users[0].id)
```

#### Success Criteria
- [x] TypeScript types match schema exactly
- [x] API client is type-safe
- [x] Works with popular frameworks (React, Vue)

---

## Sprint 11: Directives & Validation

**Goal**: Schema directives for validation and behavior.

### Tasks

#### Parser Support
- [ ] Parse `@email`, `@url`, `@phone`
- [ ] Parse `@min`, `@max`, `@pattern`
- [ ] Parse `@private`, `@public`
- [ ] Parse `@fulltext`, `@indexed`

#### Validation Logic
- [ ] Email format validation
- [ ] URL format validation
- [ ] Pattern matching (regex)
- [ ] Min/max for numbers

#### Code Generation
- [ ] Generate validation functions
- [ ] Apply validation in API handlers
- [ ] Error messages for validation failures

#### Access Control
- [ ] `@private` fields excluded from API responses
- [ ] `@admin_only` fields require auth (stub)

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
- [x] API returns 400 with helpful errors
- [x] Private fields never serialized

---

## Sprint 12: Computed Fields

**Goal**: Support computed/derived fields.

### Tasks

#### Parser
- [ ] Parse `@computed` directive
- [ ] Identify computed field dependencies

#### Code Generation
- [ ] Generate trait for computed fields
- [ ] Generate stub implementation
- [ ] Client-side computation by default

#### Runtime
- [ ] Compute on access (lazy)
- [ ] Cache results (optional)
- [ ] Include in API responses

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
  fn full_name(first: &str, last: &str) -> String;
  fn post_count(posts: &[Post]) -> u32;
}
```

#### Success Criteria
- [x] Computed fields work client-side
- [x] Trait system allows customization
- [x] API includes computed fields in responses

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

## Sprint 14: Query Optimization

**Goal**: Optimize query performance.

### Tasks

#### Columnar Scanning
- [ ] SIMD operations for numeric filters
- [ ] Batch processing (1024 rows at a time)
- [ ] Early termination for limits

#### Index Improvements
- [ ] B-tree index for range queries
- [ ] Composite indexes
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
- [x] Meet all performance targets
- [x] Efficient index usage
- [x] Optimal query plans

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
