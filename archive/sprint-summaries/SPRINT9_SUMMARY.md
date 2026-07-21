# Sprint 9: REST API Generation - Executive Summary

## What We're Building

Auto-generate a production-ready REST API from ForgeDB schemas with full CRUD operations, query parameters, validation, and type safety.

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Schema Definition                         │
│                                                                  │
│  User {                                                          │
│    id: +uuid                                                     │
│    email: ^&string @email                                        │
│    name: string                                                  │
│    age: u32 @min(0) @max(150)                                   │
│  }                                                               │
└─────────────────────────────────────────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Code Generation (Sprint 9)                  │
│                                                                  │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────────┐  │
│  │  HTTP Server   │  │  CRUD Handlers │  │ Query Parameters │  │
│  │   (Axum)       │  │  (Generic)     │  │  (Parse & Validate)│ │
│  └────────────────┘  └────────────────┘  └──────────────────┘  │
│                                                                  │
│  ┌────────────────┐  ┌────────────────────────────────────────┐ │
│  │  Validation    │  │    Generated Router & Handlers          │ │
│  │  (HTTP)        │  │    (Model-specific implementations)     │ │
│  └────────────────┘  └────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Generated REST API                        │
│                                                                  │
│  GET    /api/users              → List users                    │
│  GET    /api/users/{id}         → Get user by ID               │
│  POST   /api/users              → Create user                   │
│  PUT    /api/users/{id}         → Update user                   │
│  DELETE /api/users/{id}         → Delete user                   │
│                                                                  │
│  Query: ?email=x@y.com&sort=age&limit=50                       │
└─────────────────────────────────────────────────────────────────┘
```

## Architecture: 5 Parallel Components

```
┌──────────────────────────────────────────────────────────────────┐
│                         PHASE 1: Foundation                       │
│                      (All tasks in parallel)                      │
└──────────────────────────────────────────────────────────────────┘

┌───────────────┐  ┌───────────────┐  ┌───────────────┐  ┌─────────┐
│  Task 1       │  │  Task 2       │  │  Task 3       │  │  Task 4 │
│               │  │               │  │               │  │         │
│ http-server   │  │  crud-api     │  │ query-params  │  │validation│
│               │  │               │  │               │  │         │
│ • Axum setup  │  │ • List        │  │ • Filter      │  │• Body   │
│ • Routing     │  │ • Get         │  │ • Sort        │  │• Status │
│ • JSON        │  │ • Create      │  │ • Pagination  │  │• Errors │
│ • Errors      │  │ • Update      │  │               │  │         │
│               │  │ • Delete      │  │               │  │         │
└───────────────┘  └───────────────┘  └───────────────┘  └─────────┘
        │                  │                  │                │
        └──────────────────┴──────────────────┴────────────────┘
                              ▼
                    ┌─────────────────┐
                    │    Task 5       │
                    │                 │
                    │  codegen-api    │
                    │                 │
                    │ • Generate      │
                    │   handlers      │
                    │ • Generate      │
                    │   types         │
                    │ • Generate      │
                    │   router        │
                    └─────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                         PHASE 2: Integration                      │
│                    (After Phase 1 complete)                       │
└──────────────────────────────────────────────────────────────────┘
```

## What Each Component Does

### 1. HTTP Server (crates/http-server)
**Purpose**: Foundation for serving HTTP requests

- Axum server setup
- JSON serialization/deserialization
- Error response formatting
- Middleware (CORS, logging)

**Key Files**:
- `server.rs` - Server lifecycle
- `routing.rs` - Route registration
- `error.rs` - Error types and responses

### 2. CRUD API (crates/crud-api)
**Purpose**: Generic CRUD operation handlers

- List all records
- Get by ID
- Create new record
- Update existing record
- Delete record (tombstone)

**Key Files**:
- `handlers.rs` - Generic trait
- `list.rs`, `get.rs`, `create.rs`, `update.rs`, `delete.rs`

### 3. Query Parameters (crates/query-params)
**Purpose**: Parse and validate query strings

- Filter: `?email=test@example.com`
- Sort: `?sort=created_at&order=desc`
- Pagination: `?limit=50&offset=100`

**Key Files**:
- `parser.rs` - Query string parsing
- `filter.rs`, `sort.rs`, `pagination.rs`

### 4. Validation (extends crates/validation)
**Purpose**: HTTP-specific validation

- Request body validation
- HTTP status code mapping
- Descriptive error messages

**Key Files**:
- `http.rs` - HTTP validation
- `status.rs` - Status code mapping
- `errors.rs` - Enhanced errors

### 5. Code Generation (extends src/codegen.rs)
**Purpose**: Generate API code from schema

- Handler functions for each model
- Request/response types (serde)
- Validation logic
- Router setup

**Key Files**:
- `api_codegen.rs` - API code generation

## Example: User Model API

### Input Schema
```
User {
  id: +uuid
  email: ^&string @email
  name: string
  age: u32 @min(0) @max(150)
}
```

### Generated Code
```rust
// generated/api/user_api.rs

#[derive(Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub name: String,
    pub age: u32,
}

#[derive(Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub name: Option<String>,
    pub age: Option<u32>,
}

pub async fn list_users(
    Query(params): Query<QueryParams>,
    State(db): State<Arc<Database>>,
) -> Result<Json<Vec<User>>, ApiError> {
    // Implementation generated
}

pub async fn get_user(
    Path(id): Path<Uuid>,
    State(db): State<Arc<Database>>,
) -> Result<Json<User>, ApiError> {
    // Implementation generated
}

pub async fn create_user(
    State(db): State<Arc<Database>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<User>, ApiError> {
    // Validation + creation generated
}

// ... update, delete
```

### Generated Router
```rust
// generated/api/router.rs

pub fn create_router(db: Arc<Database>) -> Router {
    Router::new()
        .route("/api/users", get(user_api::list_users))
        .route("/api/users", post(user_api::create_user))
        .route("/api/users/:id", get(user_api::get_user))
        .route("/api/users/:id", put(user_api::update_user))
        .route("/api/users/:id", delete(user_api::delete_user))
        .with_state(db)
}
```

## HTTP Endpoints (Generated)

| Method | Endpoint | Query Params | Body | Response |
|--------|----------|--------------|------|----------|
| GET | /api/users | filter, sort, limit, offset | - | Array |
| GET | /api/users/{id} | - | - | Object |
| POST | /api/users | - | JSON | Object |
| PUT | /api/users/{id} | - | JSON | Object |
| DELETE | /api/users/{id} | - | - | 204 |

## Query Parameter Examples

```bash
# Filter by field
GET /api/users?email=test@example.com

# Sort ascending
GET /api/users?sort=age&order=asc

# Sort descending
GET /api/users?sort=created_at&order=desc

# Pagination
GET /api/users?limit=50&offset=100

# Combine all
GET /api/users?name=John&sort=age&order=asc&limit=20&offset=0
```

## Error Responses

### 400 Bad Request (Validation Error)
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

### 404 Not Found
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "User not found",
    "details": []
  }
}
```

### 409 Conflict (Unique Constraint)
```json
{
  "error": {
    "code": "CONFLICT",
    "message": "Email already exists",
    "details": [
      {
        "field": "email",
        "constraint": "unique",
        "message": "A user with this email already exists"
      }
    ]
  }
}
```

## Implementation Timeline

### Week 1: Foundation (Phase 1)
- Day 1-2: HTTP Server setup
- Day 1-2: CRUD handlers (parallel)
- Day 1-2: Query parameters (parallel)
- Day 1-2: Validation (parallel)
- Day 3: Integration of Phase 1 components

### Week 2: Code Generation (Phase 2)
- Day 1-2: API code generation
- Day 3: Generated code testing
- Day 4: Integration tests

### Week 3: Polish & Examples
- Day 1: Example application
- Day 2: Documentation
- Day 3: Performance testing
- Day 4: Bug fixes

**Total: 3 weeks**

## Tech Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| HTTP Framework | Axum | Type-safe, performant, excellent DX |
| Async Runtime | Tokio | Industry standard, required by Axum |
| Serialization | serde + serde_json | Industry standard, derive macros |
| Middleware | Tower | Modular, composable, Axum native |
| Logging | tracing | Structured logging, async-aware |

## Success Criteria

✅ Sprint 9 is complete when:

1. **Code Quality**
   - [ ] All generated code compiles
   - [ ] All tests pass (unit + integration)
   - [ ] No clippy warnings

2. **Functionality**
   - [ ] CRUD operations work via HTTP
   - [ ] Query parameters (filter, sort, pagination) work
   - [ ] Validation returns correct status codes
   - [ ] Error messages are descriptive

3. **Performance**
   - [ ] p99 latency < 10ms
   - [ ] Throughput > 10k req/s (single core)

4. **Documentation**
   - [ ] API endpoints documented
   - [ ] Example application works
   - [ ] README updated

## Current Status

- **Sprints Completed**: 1-8, 11, 14 (partial)
- **Test Status**: 122/122 tests passing
- **Last Sprint**: Sprint 8 (Inline Structs & Fixed Arrays)
- **Ready for**: Sprint 9 implementation

## Next Steps

1. **Review this document** ✓
2. **Read SPRINT9_PREPARATION.md** for full details
3. **Read SPRINT9_QUICK_START.md** for implementation guide
4. **Create sprint-9/main branch**
5. **Begin parallel implementation** of Tasks 1-4

## Files Created

- [SPRINT9_PREPARATION.md](./SPRINT9_PREPARATION.md) - Full implementation plan
- [SPRINT9_QUICK_START.md](./SPRINT9_QUICK_START.md) - Quick reference guide
- [SPRINT9_SUMMARY.md](./SPRINT9_SUMMARY.md) - This document

---

**Status**: 📋 Prepared - Ready for Implementation
**Date**: 2025-10-13
**Sprint**: 9 (REST API Generation)
