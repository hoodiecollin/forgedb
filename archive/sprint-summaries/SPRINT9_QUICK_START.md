# Sprint 9: Quick Start Guide

## Overview

Sprint 9 adds REST API generation to ForgeDB. This guide provides a quick reference for implementation.

## Workspace Structure

```
kitchen-sink/
├── crates/
│   ├── http-server/      ← NEW (Task 1)
│   ├── crud-api/         ← NEW (Task 2)
│   ├── query-params/     ← NEW (Task 3)
│   ├── validation/       ← EXTEND (Task 4)
│   └── [existing crates]
├── src/
│   ├── codegen.rs        ← EXTEND (Task 5)
│   └── api_codegen.rs    ← NEW (Task 5)
└── SPRINT9_PREPARATION.md
```

## Implementation Order

### Phase 1: Parallel Tasks (Can run simultaneously)

**Task 1: HTTP Server (sprint-9/server)**
```bash
# Create workspace member
mkdir -p crates/http-server/src
cd crates/http-server

# Create Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "forgedb-http-server"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
EOF

# Implement server.rs, routing.rs, error.rs
```

**Task 2: CRUD API (sprint-9/crud)**
```bash
mkdir -p crates/crud-api/src
cd crates/crud-api

# Create Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "forgedb-crud-api"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = { path = "../storage" }
uuid = { version = "1.6", features = ["v4"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
EOF

# Implement handlers.rs, list.rs, get.rs, create.rs, update.rs, delete.rs
```

**Task 3: Query Parameters (sprint-9/query)**
```bash
mkdir -p crates/query-params/src
cd crates/query-params

# Create Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "forgedb-query-params"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_urlencoded = "0.7"
EOF

# Implement parser.rs, filter.rs, sort.rs, pagination.rs
```

**Task 4: Validation (sprint-9/validation)**
```bash
cd crates/validation/src

# Extend existing validation crate
# Add: http.rs, status.rs, errors.rs
```

### Phase 2: Code Generation (Depends on Phase 1)

**Task 5: API Code Generation (sprint-9/codegen)**
```bash
cd src

# Create api_codegen.rs module
# Extend codegen.rs to generate API code
```

## Key Files to Create

### 1. crates/http-server/src/lib.rs
```rust
mod server;
mod routing;
mod error;

pub use server::Server;
pub use routing::Router;
pub use error::{ApiError, ErrorResponse};
```

### 2. crates/crud-api/src/lib.rs
```rust
mod handlers;
mod list;
mod get;
mod create;
mod update;
mod delete;

pub use handlers::{CrudHandler, CrudOperations};
```

### 3. crates/query-params/src/lib.rs
```rust
mod parser;
mod filter;
mod sort;
mod pagination;

pub use parser::QueryParams;
pub use filter::Filter;
pub use sort::{Sort, SortOrder};
pub use pagination::Pagination;
```

### 4. src/api_codegen.rs
```rust
use crate::ast::{Model, Schema};

pub fn generate_api_code(schema: &Schema) -> Vec<GeneratedFile> {
    // Generate handler functions
    // Generate request/response types
    // Generate router setup
}
```

## Testing Strategy

Each task should include comprehensive tests:

```bash
# Unit tests for each crate
cargo test --package forgedb-http-server
cargo test --package forgedb-crud-api
cargo test --package forgedb-query-params
cargo test --package forgedb-validation

# Integration tests
cargo test --test api_integration

# End-to-end example
cargo run --example sprint9_api
```

## Example Schema for Testing

```
User {
  id: +uuid
  email: ^&string @email
  name: string
  age: u32 @min(0) @max(150)
  created_at: +timestamp
}

Post {
  id: +uuid
  title: string
  content: string
  author: *User
  created_at: +timestamp
}
```

## Expected Generated API

```
GET    /api/users              → List users
GET    /api/users/{id}         → Get user by ID
POST   /api/users              → Create user
PUT    /api/users/{id}         → Update user
DELETE /api/users/{id}         → Delete user

GET    /api/posts              → List posts
GET    /api/posts/{id}         → Get post by ID
POST   /api/posts              → Create post
PUT    /api/posts/{id}         → Update post
DELETE /api/posts/{id}         → Delete post

GET    /api/users/{id}/posts   → Get user's posts (relation)
```

## Query Parameters

```bash
# Filter
GET /api/users?email=test@example.com

# Sort
GET /api/users?sort=created_at&order=desc

# Pagination
GET /api/users?limit=50&offset=100

# Combine
GET /api/users?name=John&sort=age&order=asc&limit=20
```

## Error Responses

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid email format",
    "details": [
      {
        "field": "email",
        "message": "Must be a valid email address"
      }
    ]
  }
}
```

## HTTP Status Codes

- 200 OK - Successful GET, PUT, DELETE
- 201 Created - Successful POST
- 400 Bad Request - Validation error
- 404 Not Found - Resource not found
- 409 Conflict - Unique constraint violation
- 500 Internal Server Error - Server error

## Update Workspace Cargo.toml

```toml
[workspace]
members = [
    ".",
    "crates/storage",
    "crates/types",
    "crates/validation",
    "crates/tests",
    "crates/cli",
    "crates/watcher",
    "crates/wal",
    "crates/http-server",      # NEW
    "crates/crud-api",         # NEW
    "crates/query-params",     # NEW
]

[dependencies]
# Add new dependencies
forgedb-http-server = { path = "crates/http-server" }
forgedb-crud-api = { path = "crates/crud-api" }
forgedb-query-params = { path = "crates/query-params" }
```

## Branch Strategy

```bash
# Main sprint branch
git checkout -b sprint-9/main

# Feature branches (parallel development)
git checkout -b sprint-9/server
git checkout -b sprint-9/crud
git checkout -b sprint-9/query
git checkout -b sprint-9/validation
git checkout -b sprint-9/codegen
```

## Success Checklist

- [ ] HTTP server setup complete (Axum)
- [ ] CRUD handlers implemented
- [ ] Query parameter parsing works
- [ ] Request validation works
- [ ] API code generation works
- [ ] Generated code compiles
- [ ] Integration tests pass
- [ ] Example application works
- [ ] All 5 workspace members in Cargo.toml
- [ ] Documentation updated

## Performance Targets

- REST endpoint p99: < 10ms
- Throughput: > 10k req/s (single core)
- JSON serialization: < 1ms
- Query parsing: < 0.1ms

## Next Command to Run

```bash
# Start implementation
git checkout -b sprint-9/main

# Create workspace structure
mkdir -p crates/{http-server,crud-api,query-params}/src

# Update workspace Cargo.toml
# (Add new members)

# Begin parallel implementation of Tasks 1-4
```

## Resources

- Full details: [SPRINT9_PREPARATION.md](./SPRINT9_PREPARATION.md)
- Sprint plan: [SPRINT_PLAN.md](./SPRINT_PLAN.md)
- Axum docs: https://docs.rs/axum/
- serde docs: https://serde.rs/

---

**Ready for Implementation** 🚀
