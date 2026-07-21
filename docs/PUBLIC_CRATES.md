# ForgeDB Public Crates

**Last Updated:** October 2025  
**Audience:** Library users, generated code developers

## Table of Contents

- [Overview](#overview)
- [Runtime Library Guide](#runtime-library-guide)
- [How Generated Code Uses Them](#how-generated-code-uses-them)
- [Integration Patterns](#integration-patterns)
- [Dependency Graph](#dependency-graph)
- [Version Policy](#version-policy)
- [Publishing Process](#publishing-process)

---

## Overview

ForgeDB's public crates are the **runtime libraries** that generated code depends on. These crates provide:

- Columnar storage engine
- Write-Ahead Log (WAL)
- CRUD operations
- HTTP server infrastructure
- Query optimization
- Full-text search
- Data compaction

### Design Philosophy

Public crates are designed to be:

✅ **Standalone**: Usable without ForgeDB tooling  
✅ **Stable**: Semantic versioning guarantees  
✅ **Production-ready**: Battle-tested with comprehensive tests  
✅ **Well-documented**: Full API docs and examples  
✅ **Zero tooling dependencies**: No dependencies on code generation crates

---

## Runtime Library Guide

### Foundation Layer

#### forgedb-types

**Purpose**: Core type definitions and traits.

**Status**: ⚠️ Under development

**Key Exports**:
```rust
// Type system traits
pub trait TypedColumn { /* ... */ }
pub trait Serializable { /* ... */ }

// Common types
pub use uuid::Uuid;
pub type Timestamp = i64;
pub type RowId = u64;
```

**When to use**:
- Implementing custom types
- Extending ForgeDB type system
- Type-safe operations

**Dependencies**: None (foundation crate)

---

### Storage Layer

#### forgedb-storage

**Purpose**: Columnar storage engine with memory-mapped files.

**Version**: 0.1.0

**Key Exports**:
```rust
// Main database interface
pub struct Database { /* ... */ }
pub struct Manifest { /* ... */ }

// Column storage
pub struct FixedColumn { /* ... */ }
pub struct VariableColumn { /* ... */ }
pub struct Tombstones { /* ... */ }

// Column metadata
pub struct ColumnMetadata { /* ... */ }
pub enum ColumnType {
    U64,
    I64,
    F64,
    String,
    Uuid,
}
```

**Usage Example**:
```rust
use forgedb_storage::{Database, ColumnMetadata, ColumnType};
use std::path::PathBuf;

// Open database
let mut db = Database::open(PathBuf::from("./data"))?;

// Define schema
db.set_columns(vec![
    ColumnMetadata {
        name: "id".to_string(),
        column_type: ColumnType::U64,
        column_index: 0,
    },
    ColumnMetadata {
        name: "email".to_string(),
        column_type: ColumnType::String,
        column_index: 0,
    },
]);

db.save_manifest()?;
```

**When to use**:
- Low-level storage operations
- Custom database implementations
- Performance-critical code

**Dependencies**:
- `forgedb-wal` (optional, for transactions)
- `serde` (serialization)
- `uuid` (UUID support)

**Documentation**: [forgedb-storage README](../crates/storage/README.md)

---

#### forgedb-wal

**Purpose**: Write-Ahead Log for ACID guarantees.

**Version**: 0.1.0

**Key Exports**:
```rust
// WAL management
pub struct WalManager { /* ... */ }
pub struct Transaction { /* ... */ }
pub type TransactionId = u64;

// Operations
pub enum WalOperation {
    Insert,
    Update,
    Delete,
}

pub enum WalValue {
    U64(u64),
    String(String),
    Uuid(uuid::Uuid),
    // ... other types
}

// Durability control
pub enum FsyncPolicy {
    Never,
    Always,
    Periodic(Duration),
}
```

**Usage Example**:
```rust
use forgedb_storage::{Database, FsyncPolicy};
use forgedb_wal::{Transaction, WalValue};

// Open database with WAL
let mut db = Database::open_with_wal(
    PathBuf::from("./data"),
    FsyncPolicy::Always
)?;

// Create transaction
let mut txn = Transaction::new();
txn.insert("User", user_id, vec![
    ("email".to_string(), WalValue::String("user@example.com".to_string())),
    ("age".to_string(), WalValue::U64(25)),
]);

// Commit
if let Some(wal) = db.wal_mut() {
    txn.commit(wal)?;
}
```

**When to use**:
- Transactional writes
- ACID guarantees
- Crash recovery
- Point-in-time recovery

**Dependencies**:
- `serde` (serialization)
- `uuid` (transaction IDs)
- `crc32fast` (checksums)

**Documentation**: See crate-level docs

---

### API & Query Layer

#### forgedb-crud-api

**Purpose**: High-level CRUD operations.

**Version**: 0.1.0

**Key Exports**:
```rust
// CRUD operations
pub trait CrudOperations {
    fn insert(&mut self, data: Self::InsertData) -> Result<Self::Id>;
    fn find_by_id(&self, id: Self::Id) -> Result<Option<Self::Model>>;
    fn update(&mut self, id: Self::Id, data: Self::UpdateData) -> Result<()>;
    fn delete(&mut self, id: Self::Id) -> Result<()>;
}

// Query builders
pub struct QueryBuilder<T> { /* ... */ }
pub struct FilterExpression { /* ... */ }
```

**Usage Example**:
```rust
use forgedb_crud_api::*;

// Generated code uses this
impl CrudOperations for UserTable {
    type Id = uuid::Uuid;
    type Model = User;
    type InsertData = UserInsert;
    type UpdateData = UserUpdate;
    
    fn insert(&mut self, data: UserInsert) -> Result<Uuid> {
        // Implementation generated from schema
    }
}
```

**When to use**:
- High-level database operations
- Generated CRUD APIs
- Application business logic

**Dependencies**:
- `forgedb-storage`
- `serde`
- `uuid`

**Documentation**: See crate-level docs

---

#### forgedb-query-params

**Purpose**: HTTP query parameter parsing.

**Version**: 0.1.0

**Key Exports**:
```rust
// Query parameter types
#[derive(Deserialize)]
pub struct PaginationParams {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Deserialize)]
pub struct SortParams {
    pub sort_by: Option<String>,
    pub order: Option<SortOrder>,
}

pub enum SortOrder {
    Asc,
    Desc,
}

// Filter parsing
pub struct FilterParser { /* ... */ }
```

**Usage Example**:
```rust
use forgedb_query_params::*;
use axum::extract::Query;

async fn list_users(
    Query(pagination): Query<PaginationParams>,
    Query(sort): Query<SortParams>,
) -> impl IntoResponse {
    let page = pagination.page.unwrap_or(1);
    let limit = pagination.limit.unwrap_or(20);
    
    // Query database with params
}
```

**When to use**:
- HTTP API query parsing
- REST endpoint implementation
- Generated API handlers

**Dependencies**:
- `serde` (deserialization)

**Documentation**: See crate-level docs

---

#### forgedb-query-optimization

**Purpose**: SIMD-optimized query execution.

**Version**: 0.1.0

**Key Exports**:
```rust
// Query optimization
pub struct QueryPlan { /* ... */ }
pub struct QueryOptimizer { /* ... */ }

// SIMD operations
pub fn filter_u64_simd(column: &[u64], predicate: &Predicate) -> Vec<usize>;
pub fn aggregate_sum_simd(column: &[u64]) -> u64;
```

**Usage Example**:
```rust
use forgedb_query_optimization::*;

// Optimize and execute query
let optimizer = QueryOptimizer::new();
let plan = optimizer.optimize(query)?;
let results = plan.execute(&database)?;
```

**When to use**:
- Query performance optimization
- Large dataset operations
- Analytical queries

**Dependencies**:
- `forgedb-storage`
- `forgedb-types`

**Documentation**: [forgedb-query-optimization README](../crates/query-optimization/README.md)

---

#### forgedb-fulltext

**Purpose**: Full-text search capabilities.

**Version**: 0.1.0

**Key Exports**:
```rust
// Full-text search
pub struct FullTextIndex { /* ... */ }
pub struct SearchQuery { /* ... */ }
pub struct SearchResult { /* ... */ }

// Tokenization
pub trait Tokenizer {
    fn tokenize(&self, text: &str) -> Vec<String>;
}
```

**Usage Example**:
```rust
use forgedb_fulltext::*;

// Create full-text index
let mut index = FullTextIndex::new();
index.add_document(doc_id, "The quick brown fox")?;

// Search
let results = index.search("quick fox")?;
```

**When to use**:
- Text search features
- Content discovery
- Search endpoints

**Dependencies**:
- `uuid`
- `regex`

**Documentation**: [forgedb-fulltext README](../crates/fulltext/README.md)

---

### Server & Infrastructure

#### forgedb-http-server

**Purpose**: Production HTTP server infrastructure.

**Version**: 0.1.0

**Key Exports**:
```rust
// Server
pub struct Server { /* ... */ }
pub struct ServerConfig { /* ... */ }

// TLS/HTTPS
pub struct TlsConfig { /* ... */ }
pub async fn serve_https(app: Router, addr: SocketAddr, tls: TlsConfig);

// Authentication
pub trait AuthHook {
    fn authenticate(&self, req: &Request) -> Result<AuthContext, Response>;
}
pub struct ApiKeyAuthHook { /* ... */ }
pub struct JwtAuthHook { /* ... */ }

// Rate limiting
pub struct RateLimiter { /* ... */ }
pub struct RateLimitConfig { /* ... */ }

// Caching
pub struct ResponseCache { /* ... */ }
pub struct CacheConfig { /* ... */ }

// Health checks
pub fn health_router() -> Router;
pub fn init_health_check();

// Metrics
pub fn metrics_router() -> Router;
pub fn record_http_request(method: &str, path: &str, status: u16, duration: f64);

// Re-exports from axum
pub use axum::{
    Router, routing::{get, post, put, delete},
    extract::{State, Query, Path, Json},
    response::{IntoResponse, Response},
    http::{StatusCode, HeaderMap},
};
```

**Usage Example**:
```rust
use forgedb_http_server::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Initialize
    Server::init_tracing();
    init_health_check();

    // Setup auth
    let auth = Arc::new(ApiKeyAuthHook::new(vec!["secret".to_string()]));

    // Build app
    let app = Router::new()
        .route("/api/users", get(list_users))
        .merge(health_router())
        .merge(metrics_router())
        .layer(axum::middleware::from_fn(move |req, next| {
            let hook = auth.clone();
            auth_middleware(hook, req, next)
        }));

    // Start server
    Server::new().serve(app).await.unwrap();
}
```

**When to use**:
- HTTP API servers
- REST API implementation
- Production deployments

**Dependencies**:
- `axum` (web framework)
- `axum-server` (TLS support)
- `tokio` (async runtime)
- `tower` (middleware)
- `tower-http` (HTTP middleware)
- `prometheus` (metrics)
- `rustls` (TLS)
- `tracing` (logging)

**Documentation**: [forgedb-http-server README](../crates/http-server/README.md)

---

#### forgedb-compaction

**Purpose**: Background data compaction.

**Version**: 0.1.0

**Key Exports**:
```rust
// Compaction
pub struct CompactionManager { /* ... */ }
pub struct CompactionConfig { /* ... */ }

// Scheduling
pub enum CompactionTrigger {
    Manual,
    Threshold(f64),  // Tombstone ratio
    Periodic(Duration),
}
```

**Usage Example**:
```rust
use forgedb_compaction::*;

let config = CompactionConfig {
    trigger: CompactionTrigger::Threshold(0.3), // 30% deleted
    ..Default::default()
};

let mut manager = CompactionManager::new(config);
manager.compact(&mut database)?;
```

**When to use**:
- Space reclamation
- Performance optimization
- Maintenance tasks

**Dependencies**:
- `serde`
- `chrono`

**Documentation**: [forgedb-compaction README](../crates/compaction/README.md)

---

#### forgedb-ffi

**Purpose**: Foreign Function Interface for C/C++ integration.

**Version**: 0.1.0

**Key Exports**:
```rust
// C-compatible types
#[repr(C)]
pub struct CDatabase { /* ... */ }

// C API functions
#[no_mangle]
pub extern "C" fn forgedb_open(path: *const c_char) -> *mut CDatabase;

#[no_mangle]
pub extern "C" fn forgedb_close(db: *mut CDatabase);

#[no_mangle]
pub extern "C" fn forgedb_insert(/* ... */) -> c_int;
```

**Usage Example** (C):
```c
#include "forgedb.h"

int main() {
    CDatabase* db = forgedb_open("./data");
    forgedb_insert(db, /* ... */);
    forgedb_close(db);
    return 0;
}
```

**When to use**:
- C/C++ integration
- Language bindings
- Legacy system integration

**Dependencies**:
- `forgedb-storage`
- `forgedb-types`
- `libc`

**Documentation**: [forgedb-ffi README](../crates/ffi/README.md)

---

## How Generated Code Uses Them

### Schema Example

```
User {
  id: +uuid
  email: ^&string
  username: ^&string
  created_at: ^timestamp
  
  @index(email)
  @pattern(email, "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$")
}
```

### Generated Code Structure

```rust
// Generated model
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub created_at: i64,
}

// Generated storage
pub struct UserTable {
    database: Database,
    email_index: HashMap<String, Uuid>,
}

// Generated CRUD (uses forgedb-crud-api)
impl CrudOperations for UserTable {
    type Id = Uuid;
    type Model = User;
    
    fn insert(&mut self, user: UserInsert) -> Result<Uuid> {
        // Uses forgedb-storage for data persistence
        // Uses forgedb-wal for transactions
        // Updates indexes
    }
    
    fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        // Uses email_index for O(1) lookup
        // Uses forgedb-storage to load data
    }
}

// Generated HTTP API (uses forgedb-http-server)
pub fn user_routes() -> Router {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/:id", get(get_user).put(update_user).delete(delete_user))
        // Uses forgedb-query-params for query parsing
        // Uses forgedb-http-server for auth, rate limiting, caching
}

async fn create_user(
    State(db): State<Arc<Database>>,
    Json(data): Json<UserInsert>,
) -> Result<Json<User>, StatusCode> {
    // Uses generated CRUD operations
    let id = db.users.insert(data)?;
    let user = db.users.find_by_id(id)?;
    Ok(Json(user.unwrap()))
}
```

---

## Integration Patterns

### Pattern 1: Standalone Storage

Use ForgeDB storage without HTTP server:

```rust
use forgedb_storage::*;
use forgedb_crud_api::*;

fn main() {
    let mut db = Database::open("./data").unwrap();
    
    // Direct storage operations
    let mut users = UserTable::new(db);
    let user_id = users.insert(UserInsert {
        email: "test@example.com".to_string(),
        username: "testuser".to_string(),
    }).unwrap();
    
    let user = users.find_by_id(user_id).unwrap();
}
```

### Pattern 2: Custom HTTP Server

Use storage with custom server framework:

```rust
use forgedb_storage::*;
use warp::Filter;

#[tokio::main]
async fn main() {
    let db = Database::open("./data").unwrap();
    let db = Arc::new(Mutex::new(db));
    
    let route = warp::path("users")
        .and(warp::any().map(move || db.clone()))
        .and_then(handle_users);
    
    warp::serve(route).run(([127, 0, 0, 1], 3000)).await;
}
```

### Pattern 3: Embedded Database

Embed ForgeDB in application:

```rust
use forgedb_storage::*;

pub struct MyApp {
    db: Database,
}

impl MyApp {
    pub fn new(data_path: &str) -> Self {
        Self {
            db: Database::open(data_path).unwrap(),
        }
    }
    
    pub fn get_user(&self, id: Uuid) -> Option<User> {
        // Direct database access
    }
}
```

### Pattern 4: Microservice

Full-featured microservice:

```rust
use forgedb_http_server::*;
use forgedb_storage::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    Server::init_tracing();
    init_health_check();
    
    let db = Arc::new(Database::open("./data").unwrap());
    
    let app = Router::new()
        .merge(user_routes())
        .merge(health_router())
        .merge(metrics_router())
        .with_state(db);
    
    Server::new().serve(app).await.unwrap();
}
```

---

## Dependency Graph

### Visual Dependency Tree

```
forgedb-types (foundation)
    ↓
    ├─→ forgedb-storage ←─ forgedb-wal
    │       ↓
    │   forgedb-crud-api
    │       ↓
    └─→ forgedb-http-server ←─┬─ forgedb-query-params
                              ├─ forgedb-query-optimization
                              ├─ forgedb-fulltext
                              └─ forgedb-compaction
    
forgedb-ffi (standalone)
    ├─→ forgedb-storage
    └─→ forgedb-types
```

### Dependency Matrix

| Crate | Depends On |
|-------|------------|
| forgedb-types | None |
| forgedb-storage | types, wal, serde, uuid |
| forgedb-wal | serde, uuid, crc32fast |
| forgedb-crud-api | storage, serde, uuid |
| forgedb-query-params | serde |
| forgedb-query-optimization | storage, types |
| forgedb-fulltext | uuid, regex |
| forgedb-http-server | axum, tower-http, tokio, prometheus |
| forgedb-compaction | serde, chrono |
| forgedb-ffi | storage, types, libc |

### External Dependencies

**Common across crates**:
- `serde` (1.0): Serialization
- `uuid` (1.0): UUID support
- `thiserror` (1.0): Error handling
- `anyhow` (1.0): Error context

**HTTP Server specific**:
- `axum` (0.8): Web framework
- `tokio` (1.x): Async runtime
- `tower` (0.4): Middleware
- `tower-http` (0.5): HTTP middleware

---

## Version Policy

### Semantic Versioning

All public crates follow [Semantic Versioning 2.0.0](https://semver.org/):

**MAJOR.MINOR.PATCH**

- **MAJOR**: Breaking changes to public API
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

### Version Coordination

All public crates are versioned together:

```toml
forgedb-types = "0.1.0"
forgedb-storage = "0.1.0"
forgedb-wal = "0.1.0"
# ... all at same version
```

**Rationale**:
- Simplifies dependency management
- Ensures compatibility
- Clear upgrade path

### Pre-1.0 Policy

**Current status**: All crates at 0.x.x (pre-1.0)

During pre-1.0 development:
- **MINOR** bumps MAY include breaking changes
- **PATCH** bumps are backward compatible
- API stability is not guaranteed

### Post-1.0 Policy

After 1.0 release:
- **MAJOR** bumps for breaking changes (may be infrequent)
- **MINOR** bumps for new features (every 2-3 months)
- **PATCH** bumps for bug fixes (as needed)

### Deprecation Policy

Before removing public APIs:

1. Mark as deprecated in version N
2. Provide migration guide
3. Keep deprecated API for 2-3 minor versions
4. Remove in next major version

Example:
```rust
#[deprecated(since = "0.5.0", note = "Use `new_method` instead")]
pub fn old_method() { /* ... */ }
```

---

## Publishing Process

### Prerequisites

1. All tests passing
2. Documentation updated
3. CHANGELOG.md entries added
4. Version bumped in all Cargo.toml files

### Publishing Order

Publish in dependency order to avoid failures:

```bash
# 1. Foundation
cargo publish -p forgedb-types
sleep 10  # Wait for crates.io to index

# 2. Core storage
cargo publish -p forgedb-wal
sleep 10
cargo publish -p forgedb-storage
sleep 10

# 3. API layer
cargo publish -p forgedb-crud-api
cargo publish -p forgedb-query-params
cargo publish -p forgedb-query-optimization
cargo publish -p forgedb-fulltext
sleep 10

# 4. Infrastructure
cargo publish -p forgedb-compaction
cargo publish -p forgedb-http-server
cargo publish -p forgedb-ffi
```

### Automated Publishing

See [PUBLISHING.md](./PUBLISHING.md) for automated scripts and CI/CD integration.

### Verification

After publishing:

```bash
# Test installation
cargo new test-forgedb
cd test-forgedb

# Add dependency
cargo add forgedb-storage

# Verify build
cargo build

# Check documentation
cargo doc --open
```

---

## Examples & Learning Resources

### Example Applications

**Blog Platform** (comprehensive):
```bash
cargo run --example blog_platform
```

Demonstrates:
- Multi-model schemas
- Relationships
- REST API
- TypeScript SDK
- OpenAPI docs

### Tutorials

1. **Getting Started**: Basic database setup
2. **CRUD Operations**: Insert, update, delete, query
3. **Indexes**: Creating and using indexes
4. **Transactions**: WAL and ACID guarantees
5. **HTTP API**: Building REST APIs
6. **Authentication**: Securing endpoints
7. **Performance**: Optimization techniques

### API Documentation

Full API docs available at:
- https://docs.rs/forgedb-storage
- https://docs.rs/forgedb-http-server
- (other crates)

---

## Support & Community

### Getting Help

- **GitHub Issues**: Bug reports and feature requests
- **Discussions**: Questions and community support
- **Discord**: Real-time chat (coming soon)
- **Stack Overflow**: Tag `forgedb`

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for:
- Development setup
- Code style guidelines
- Testing requirements
- Pull request process

---

## References

- [Architecture Overview](./ARCHITECTURE.md)
- [Internal Crates](./INTERNAL_CRATES.md)
- [Development Guide](./DEVELOPMENT.md)
- [Contributing Guide](./CONTRIBUTING.md)
- [Publishing Guide](./PUBLISHING.md)

---

**Last Updated**: October 2025  
**Maintained by**: ForgeDB Team
