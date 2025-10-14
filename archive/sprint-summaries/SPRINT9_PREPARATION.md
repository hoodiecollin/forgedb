# Sprint 9: REST API Generation - Implementation Preparation

## Executive Summary

Sprint 9 will auto-generate a complete REST API from ForgeDB schemas, including CRUD endpoints, query parameters, validation, and OpenAPI documentation. This sprint is highly parallelizable across 5 independent workspace members.

## Current Project Status

- **Completed Sprints**: 1-8, 11, 14 (partial)
- **Test Status**: 122/122 tests passing
- **Last Sprint**: Sprint 8 (Inline Structs & Fixed Arrays)
- **Current Branch**: main

## Sprint 9 Scope Overview

### Goal
Auto-generate a production-ready REST API from schema definitions with:
- CRUD endpoints for all models
- Query parameter parsing (filter, sort, pagination)
- Request validation with descriptive errors
- JSON serialization/deserialization
- Type-safe generated code

### Success Criteria
```bash
$ curl http://localhost:3000/api/users
$ curl -X POST http://localhost:3000/api/users \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com"}'
```

## Architecture Design

### Workspace Members (Cargo Crates)

Sprint 9 will add the following new crates to the workspace:

```
crates/
├── http-server/     # HTTP server setup (framework selection, routing)
├── crud-api/        # CRUD endpoint handlers and logic
├── query-params/    # Query parameter parsing and filtering
├── validation/      # Request validation (extends existing validation crate)
└── codegen/         # API code generation (extends main codegen)
```

### Parallel Task Structure

The orchestration metadata from SPRINT_PLAN.md shows 5 parallelizable tasks:

1. **http-server** (no dependencies)
   - HTTP framework setup (Axum recommended)
   - JSON serialization (serde)
   - Error response formatting
   - Server integration tests

2. **crud-endpoints** (no dependencies)
   - GET /api/{model} - list
   - GET /api/{model}/{id} - get by ID
   - POST /api/{model} - create
   - PUT /api/{model}/{id} - update
   - DELETE /api/{model}/{id} - delete

3. **query-params** (no dependencies)
   - Filter: ?email=x@y.com
   - Sort: ?sort=created_at&order=desc
   - Pagination: ?limit=50&offset=100
   - Query parser with validation

4. **validation** (no dependencies)
   - Request body validation
   - HTTP status codes (400, 404, 409)
   - Descriptive error messages

5. **codegen-api** (depends on all above)
   - Generate handler functions
   - Generate request/response types
   - Generate validation logic
   - Generate router setup

## Technical Decisions

### HTTP Framework Selection

**Recommendation: Axum**

Criteria for selection:
- Type safety (compile-time guarantees)
- Performance (async, zero-cost abstractions)
- Ergonomics (ease of use, good DX)
- Ecosystem (serde integration, Tower middleware)
- Maintenance (active development, community support)

**Candidates:**

1. **Axum** ⭐ RECOMMENDED
   - Pros: Type-safe extractors, Tower middleware, excellent ergonomics
   - Cons: Newer (but stable)
   - Best fit: Aligns with ForgeDB's type-safety philosophy

2. **Actix-web**
   - Pros: Battle-tested, high performance
   - Cons: More complex, macro-heavy
   - Use case: If raw performance is critical

3. **Warp**
   - Pros: Purely functional, type-safe filters
   - Cons: Steeper learning curve
   - Use case: If functional style preferred

**Decision**: Use **Axum** for Sprint 9
- Best balance of type safety, performance, and ergonomics
- Strong serde integration
- Excellent error handling
- Future-proof (tokio ecosystem)

### Serialization

**serde + serde_json**
- Industry standard
- Derive macros for generated types
- Excellent performance
- Rich ecosystem

### Async Runtime

**tokio**
- Required by Axum
- Industry standard
- Excellent performance
- Rich ecosystem

## Implementation Plan

### Phase 1: Foundation (http-server, crud-api, query-params, validation)

These 4 tasks can run **in parallel**:

#### Task 1: http-server
```
Branch: sprint-9/server
Files:
  crates/http-server/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs          # Public API
    │   ├── server.rs       # Server setup
    │   ├── routing.rs      # Route registration
    │   ├── error.rs        # Error types and responses
    │   └── middleware.rs   # Middleware (logging, CORS, etc.)
    └── tests/
        └── integration.rs

Dependencies:
  - axum
  - tokio
  - serde
  - serde_json
  - tower
  - tower-http (for middleware)

Deliverables:
  - Basic Axum server setup
  - JSON request/response handling
  - Error response formatting
  - Server lifecycle management
  - Integration tests
```

#### Task 2: crud-endpoints (crud-api)
```
Branch: sprint-9/crud
Files:
  crates/crud-api/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs          # Public API
    │   ├── handlers.rs     # Generic CRUD handlers
    │   ├── list.rs         # List handler logic
    │   ├── get.rs          # Get by ID logic
    │   ├── create.rs       # Create logic
    │   ├── update.rs       # Update logic
    │   └── delete.rs       # Delete logic
    └── tests/
        └── unit.rs

Dependencies:
  - forgedb-storage
  - uuid
  - serde
  - serde_json

Deliverables:
  - Generic CRUD handler traits
  - Implementation for each operation
  - Handler tests
```

#### Task 3: query-params
```
Branch: sprint-9/query
Files:
  crates/query-params/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs          # Public API
    │   ├── parser.rs       # Query string parser
    │   ├── filter.rs       # Filter parameter handling
    │   ├── sort.rs         # Sort parameter handling
    │   ├── pagination.rs   # Limit/offset handling
    │   └── validation.rs   # Query validation
    └── tests/
        └── unit.rs

Dependencies:
  - serde
  - serde_urlencoded

Deliverables:
  - Query parameter parsing
  - Type-safe filter representation
  - Sort and pagination support
  - Validation and error handling
```

#### Task 4: validation (extends existing)
```
Branch: sprint-9/validation
Files:
  crates/validation/src/
    ├── http.rs             # HTTP-specific validation
    ├── status.rs           # HTTP status code mapping
    └── errors.rs           # Enhanced error messages

Dependencies:
  - existing forgedb-validation
  - http (for status codes)

Deliverables:
  - Request body validation
  - HTTP status code mapping
  - Detailed error messages
  - Validation tests
```

### Phase 2: Code Generation (codegen-api)

**Depends on**: All Phase 1 tasks

```
Branch: sprint-9/codegen
Files:
  src/codegen.rs (extend existing)
  src/api_codegen.rs (new module)

Responsibilities:
  - Generate handler functions for each model
  - Generate request/response types (serde)
  - Generate validation logic
  - Generate router setup
  - Generate OpenAPI spec (bonus)

Deliverables:
  - API code generation methods
  - Generated code compiles and passes tests
  - Integration with existing codegen
```

## Generated Code Structure

For a schema like:
```
User {
  id: +uuid
  email: ^&string @email
  name: string
  age: u32 @min(0) @max(150)
}
```

Generated API code:
```
generated/
├── api/
│   ├── mod.rs                  # Module exports
│   ├── user_api.rs             # User CRUD handlers
│   ├── user_types.rs           # Request/response types
│   └── router.rs               # Router setup
├── models/
│   └── user_storage.rs         # Existing storage code
└── main.rs                     # Server entry point
```

## Testing Strategy

### Unit Tests
- Each crate has comprehensive unit tests
- Test individual components in isolation
- Mock dependencies where needed

### Integration Tests
- End-to-end API tests
- Test with real HTTP requests
- Verify all CRUD operations
- Test error cases

### Generated Code Tests
- Verify generated API compiles
- Test generated handlers
- Validate error responses
- Test query parameters

## Example API Endpoints

For a User model:

| Method | Endpoint | Description | Body |
|--------|----------|-------------|------|
| GET | /api/users | List all users | - |
| GET | /api/users?email=test@x.com | Filter users | - |
| GET | /api/users?sort=created_at&order=desc | Sort users | - |
| GET | /api/users?limit=50&offset=100 | Paginate | - |
| GET | /api/users/{id} | Get user by ID | - |
| POST | /api/users | Create user | JSON |
| PUT | /api/users/{id} | Update user | JSON |
| DELETE | /api/users/{id} | Delete user | - |

## Error Response Format

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

## Dependencies to Add

New dependencies for Sprint 9:

```toml
# crates/http-server/Cargo.toml
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"

# crates/query-params/Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_urlencoded = "0.7"

# Main workspace
[dependencies]
http = "1.0"  # For HTTP types
```

## Performance Targets

- REST endpoint p99 latency: < 10ms
- Throughput: > 10k req/s (single core)
- JSON serialization overhead: < 1ms
- Query parameter parsing: < 0.1ms

## Migration from Sprint 8

Sprint 8 completed inline structs and fixed arrays. Sprint 9 builds on top:

- Use existing storage layer (no changes needed)
- Use existing validation (extend for HTTP)
- Use existing codegen (extend for API)
- Serialize structs to JSON automatically (serde)

## Open Questions

1. **OpenAPI Generation**: Include in Sprint 9 or defer to Sprint 13?
   - Recommendation: Basic version in Sprint 9, full documentation in Sprint 13

2. **Authentication/Authorization**: Include in Sprint 9?
   - Recommendation: Defer to Sprint 20 (Production Readiness)

3. **Relation Endpoints**: Support `/api/users/{id}/posts`?
   - Recommendation: Yes, include basic relation traversal

4. **WebSocket Support**: Real-time updates?
   - Recommendation: Defer to future sprint

5. **GraphQL Alternative**: Support GraphQL instead of/in addition to REST?
   - Recommendation: Defer to future sprint

## Risk Assessment

### Low Risk
- HTTP framework selection (Axum is proven)
- JSON serialization (serde is standard)
- Basic CRUD operations (well-defined)

### Medium Risk
- Query parameter complexity (filter expressions)
- Error message quality (need good UX)
- Generated code size (potential for bloat)

### Mitigation Strategies
- Start with simple query parameters, iterate
- Invest in error message templates
- Use macros to reduce generated code size
- Comprehensive testing at each phase

## Next Steps

1. **Create Sprint 9 branch**: `git checkout -b sprint-9/main`
2. **Create workspace members**: Add new crates to Cargo.toml
3. **Set up orchestration**: Prepare for parallel development
4. **Implement Phase 1**: 4 parallel tasks (http-server, crud-api, query-params, validation)
5. **Implement Phase 2**: Code generation (depends on Phase 1)
6. **Integration**: Combine all components
7. **Testing**: Comprehensive end-to-end tests
8. **Example**: Create working demo application

## Estimated Timeline

- Phase 1 (parallel): 1-2 weeks
- Phase 2 (codegen): 1 week
- Integration & Testing: 1 week
- Total: 3-4 weeks

## Success Metrics

✅ Sprint 9 is complete when:
- [ ] All 5 workspace members implemented
- [ ] Generated API code compiles
- [ ] All CRUD operations work via HTTP
- [ ] Query parameters (filter, sort, pagination) work
- [ ] Validation returns appropriate status codes
- [ ] Integration tests pass
- [ ] Example application demonstrates all features
- [ ] Documentation updated

## Resources

- [Axum Documentation](https://docs.rs/axum/)
- [serde Documentation](https://serde.rs/)
- [HTTP Status Codes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status)
- [REST API Best Practices](https://restfulapi.net/)

---

**Status**: Ready for Implementation
**Prepared**: 2025-10-13
**Sprint**: 9 (REST API Generation)
