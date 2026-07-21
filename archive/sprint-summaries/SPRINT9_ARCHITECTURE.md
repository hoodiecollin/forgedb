# Sprint 9: Architecture Diagrams

## System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              CLIENT LAYER                                │
│                                                                           │
│  curl / Postman / Browser                                                │
│  HTTP Requests: GET /api/users?sort=age&limit=50                        │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           HTTP SERVER LAYER                              │
│                        (crates/http-server)                              │
│                                                                           │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                         Axum Router                               │   │
│  │                                                                   │   │
│  │  Route: GET /api/users      → Handler: list_users()             │   │
│  │  Route: GET /api/users/:id  → Handler: get_user()               │   │
│  │  Route: POST /api/users     → Handler: create_user()            │   │
│  │  Route: PUT /api/users/:id  → Handler: update_user()            │   │
│  │  Route: DELETE /api/users/:id → Handler: delete_user()          │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                           │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                      Middleware Stack                             │   │
│  │                                                                   │   │
│  │  • CORS                                                          │   │
│  │  • Logging (tracing)                                             │   │
│  │  • Error handling                                                │   │
│  │  • JSON serialization                                            │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         REQUEST PROCESSING LAYER                         │
│                                                                           │
│  ┌──────────────────┐  ┌──────────────────┐  ┌────────────────────┐    │
│  │  Query Params    │  │   Validation     │  │   CRUD Handlers    │    │
│  │  (query-params)  │  │  (validation)    │  │   (crud-api)       │    │
│  │                  │  │                  │  │                    │    │
│  │  • Parse filter  │  │  • Body validate │  │  • list()          │    │
│  │  • Parse sort    │  │  • Constraints   │  │  • get()           │    │
│  │  • Parse limit   │  │  • Status codes  │  │  • create()        │    │
│  │  • Parse offset  │  │  • Error format  │  │  • update()        │    │
│  │                  │  │                  │  │  • delete()        │    │
│  └──────────────────┘  └──────────────────┘  └────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          STORAGE LAYER                                   │
│                      (existing: crates/storage)                          │
│                                                                           │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                       Database Storage                            │   │
│  │                                                                   │   │
│  │  • Columnar storage (memory-mapped)                              │   │
│  │  • Indexes (hash, btree, composite)                              │   │
│  │  • WAL (write-ahead log)                                         │   │
│  │  • Transactions                                                  │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Code Generation Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            SCHEMA DEFINITION                             │
│                                                                           │
│  schema.forge:                                                            │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │ User {                                                           │    │
│  │   id: +uuid                                                      │    │
│  │   email: ^&string @email                                         │    │
│  │   name: string                                                   │    │
│  │   age: u32 @min(0) @max(150)                                    │    │
│  │ }                                                                │    │
│  └─────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        PARSER (existing)                                 │
│                                                                           │
│  Lexer → Parser → AST → Validation                                      │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      CODE GENERATOR (Sprint 9)                           │
│                          src/api_codegen.rs                              │
│                                                                           │
│  For each Model in Schema:                                               │
│    ├─→ Generate Request Types                                            │
│    ├─→ Generate Response Types                                           │
│    ├─→ Generate Handler Functions                                        │
│    ├─→ Generate Validation Logic                                         │
│    └─→ Generate Router Setup                                             │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          GENERATED CODE                                  │
│                                                                           │
│  generated/                                                              │
│  ├── api/                                                                │
│  │   ├── user_api.rs       ← Handler functions                          │
│  │   ├── user_types.rs     ← Request/Response types                     │
│  │   ├── router.rs         ← Router setup                               │
│  │   └── mod.rs            ← Module exports                             │
│  └── models/                                                             │
│      └── user_storage.rs   ← Existing storage (Sprint 1-8)              │
└─────────────────────────────────────────────────────────────────────────┘
```

## Request Flow (Example: Create User)

```
1. HTTP Request
   POST /api/users
   Content-Type: application/json
   Body: {"email": "test@example.com", "name": "John", "age": 25}
        │
        ▼
2. Axum Router (http-server)
   Matches route: POST /api/users → create_user()
        │
        ▼
3. Extract & Deserialize (serde)
   Json(req): Json<CreateUserRequest>
        │
        ▼
4. Validate Request (validation)
   • Check email format (@email constraint)
   • Check age range (@min(0) @max(150))
   • Return 400 if invalid
        │
        ▼
5. Execute Handler (crud-api)
   create_user(db, req) {
     db.users.insert(req.email, req.name, req.age)
   }
        │
        ▼
6. Storage Layer (storage)
   • Insert into columnar storage
   • Update indexes (email is unique)
   • Write to WAL
   • Return User struct
        │
        ▼
7. Serialize Response (serde)
   Json(user) → {"id": "...", "email": "...", ...}
        │
        ▼
8. HTTP Response
   201 Created
   Content-Type: application/json
   Body: {"id": "123e4567-...", "email": "test@example.com", ...}
```

## Error Flow (Example: Validation Error)

```
1. HTTP Request
   POST /api/users
   Body: {"email": "invalid-email", "age": 200}
        │
        ▼
2. Axum Router → create_user()
        │
        ▼
3. Validate Request (validation)
   ✗ email: Invalid format
   ✗ age: Must be <= 150
        │
        ▼
4. Build Error Response
   ApiError::ValidationError {
     fields: [
       ("email", "Must be a valid email address"),
       ("age", "Must be less than or equal to 150")
     ]
   }
        │
        ▼
5. Error Handler (http-server)
   Map to HTTP response:
   • Status: 400 Bad Request
   • Body: JSON error details
        │
        ▼
6. HTTP Response
   400 Bad Request
   Body: {
     "error": {
       "code": "VALIDATION_ERROR",
       "message": "Invalid request data",
       "details": [
         {"field": "email", "message": "Must be a valid email address"},
         {"field": "age", "message": "Must be less than or equal to 150"}
       ]
     }
   }
```

## Module Dependencies

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Dependency Graph                                 │
└─────────────────────────────────────────────────────────────────────────┘

                    ┌─────────────────────┐
                    │   api_codegen.rs    │
                    │  (Code Generator)   │
                    └─────────────────────┘
                              │
                              │ depends on
                              ▼
        ┌─────────────────────────────────────────────┐
        │                                             │
        ▼                                             ▼
┌───────────────┐                           ┌────────────────┐
│  http-server  │                           │   crud-api     │
│               │                           │                │
│  • Server     │                           │  • Handlers    │
│  • Routing    │                           │  • CRUD ops    │
│  • Errors     │                           │                │
└───────────────┘                           └────────────────┘
        │                                             │
        │                                             │
        ▼                                             ▼
┌───────────────┐                           ┌────────────────┐
│ query-params  │                           │   validation   │
│               │                           │                │
│  • Parser     │                           │  • HTTP val    │
│  • Filter     │                           │  • Status      │
│  • Sort       │                           │  • Errors      │
│  • Pagination │                           │                │
└───────────────┘                           └────────────────┘
        │                                             │
        └─────────────────┬───────────────────────────┘
                          │
                          │ all depend on
                          ▼
                  ┌───────────────┐
                  │    storage    │
                  │   (existing)  │
                  │               │
                  │  • Columnar   │
                  │  • Indexes    │
                  │  • WAL        │
                  └───────────────┘
```

## Workspace Structure

```
kitchen-sink/
│
├── Cargo.toml (workspace)
│
├── src/                       ← Main forgedb crate
│   ├── lib.rs
│   ├── lexer.rs
│   ├── parser.rs
│   ├── ast.rs
│   ├── codegen.rs             ← Extends for API generation
│   └── api_codegen.rs         ← NEW: API code generator
│
└── crates/
    │
    ├── http-server/           ← NEW (Task 1)
    │   ├── Cargo.toml
    │   ├── src/
    │   │   ├── lib.rs
    │   │   ├── server.rs
    │   │   ├── routing.rs
    │   │   ├── error.rs
    │   │   └── middleware.rs
    │   └── tests/
    │
    ├── crud-api/              ← NEW (Task 2)
    │   ├── Cargo.toml
    │   ├── src/
    │   │   ├── lib.rs
    │   │   ├── handlers.rs
    │   │   ├── list.rs
    │   │   ├── get.rs
    │   │   ├── create.rs
    │   │   ├── update.rs
    │   │   └── delete.rs
    │   └── tests/
    │
    ├── query-params/          ← NEW (Task 3)
    │   ├── Cargo.toml
    │   ├── src/
    │   │   ├── lib.rs
    │   │   ├── parser.rs
    │   │   ├── filter.rs
    │   │   ├── sort.rs
    │   │   ├── pagination.rs
    │   │   └── validation.rs
    │   └── tests/
    │
    ├── validation/            ← EXTEND (Task 4)
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── http.rs        ← NEW
    │       ├── status.rs      ← NEW
    │       └── errors.rs      ← NEW
    │
    ├── storage/               ← Existing (Sprint 2-8)
    ├── types/                 ← Existing
    ├── cli/                   ← Existing (Sprint 5)
    ├── watcher/               ← Existing (Sprint 5)
    └── wal/                   ← Existing (Sprint 7)
```

## API Endpoint Structure

```
Base URL: http://localhost:3000

/api
 │
 ├── /users                    ← Model: User
 │   ├── GET                   → list_users()
 │   │   Query params:
 │   │   • ?email=X            (filter)
 │   │   • ?sort=age           (sort field)
 │   │   • &order=asc|desc     (sort order)
 │   │   • &limit=50           (pagination)
 │   │   • &offset=100         (pagination)
 │   │
 │   ├── POST                  → create_user()
 │   │   Body: CreateUserRequest
 │   │
 │   └── /{id}
 │       ├── GET               → get_user(id)
 │       ├── PUT               → update_user(id)
 │       │   Body: UpdateUserRequest
 │       └── DELETE            → delete_user(id)
 │
 ├── /posts                    ← Model: Post
 │   └── (same structure)
 │
 └── /[other-models]           ← Auto-generated for each model
     └── (same structure)
```

## Type Flow

```
Schema Definition (DSL)
        │
        ▼
    AST Types
    (Model, Field, FieldType)
        │
        ▼
Generated Rust Types
        │
        ├─→ Storage Types (Sprint 1-8)
        │   User { id: Uuid, email: String, ... }
        │
        └─→ API Types (Sprint 9)
            │
            ├─→ Request Types (for POST/PUT)
            │   CreateUserRequest { email: String, ... }
            │   UpdateUserRequest { email: Option<String>, ... }
            │
            ├─→ Response Types (for GET)
            │   User (same as storage)
            │   Vec<User> (for list)
            │
            └─→ Query Types
                QueryParams { filter, sort, pagination }
```

## Parallelization Strategy

```
Sprint 9 Tasks:

                    ┌──────────────────────┐
                    │  Sprint 9 Kickoff    │
                    └──────────────────────┘
                              │
                              ▼
        ┌─────────────────────────────────────────────┐
        │            Phase 1: Foundation               │
        │           (All tasks in parallel)            │
        └─────────────────────────────────────────────┘
                              │
        ┌─────────────┬───────┴────────┬──────────────┐
        │             │                │              │
        ▼             ▼                ▼              ▼
   ┌────────┐   ┌─────────┐   ┌──────────┐   ┌──────────┐
   │ Task 1 │   │ Task 2  │   │  Task 3  │   │  Task 4  │
   │        │   │         │   │          │   │          │
   │ HTTP   │   │  CRUD   │   │  Query   │   │Validation│
   │ Server │   │  API    │   │  Params  │   │          │
   │        │   │         │   │          │   │          │
   │ 2 days │   │ 2 days  │   │  2 days  │   │  2 days  │
   └────────┘   └─────────┘   └──────────┘   └──────────┘
        │             │                │              │
        └─────────────┴────────┬───────┴──────────────┘
                               │
                               ▼
                      ┌─────────────────┐
                      │  Integration    │
                      │     1 day       │
                      └─────────────────┘
                               │
                               ▼
        ┌─────────────────────────────────────────────┐
        │            Phase 2: Code Gen                 │
        └─────────────────────────────────────────────┘
                               │
                               ▼
                      ┌─────────────────┐
                      │    Task 5       │
                      │                 │
                      │  API Codegen    │
                      │                 │
                      │    1 week       │
                      └─────────────────┘
                               │
                               ▼
                      ┌─────────────────┐
                      │  Testing &      │
                      │  Polish         │
                      │    1 week       │
                      └─────────────────┘
```

---

**Total Time: 3 weeks**
- Week 1: Phase 1 (parallel) + Integration
- Week 2: Phase 2 (codegen)
- Week 3: Testing, examples, documentation
