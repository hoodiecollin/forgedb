# SinkDB API Generation Specification

## Overview

SinkDB automatically generates a complete REST API from the schema, including CRUD operations, relationship traversal, query parameters, validation, and OpenAPI documentation.

## Design Principles

1. **Convention over Configuration**: Sensible defaults for all routes
2. **Type Safety**: Request/response types match schema exactly
3. **Consistency**: Predictable URL patterns and behavior
4. **Self-Documenting**: OpenAPI spec generated automatically
5. **Extensibility**: Custom endpoints when needed

## URL Structure

### Pattern

```
/api/{model}/{id?}/{relation?}
```

### Examples

```
/api/users                    # Collection
/api/users/123                # Single resource
/api/users/123/posts          # Related resources
/api/users/123/posts/456      # Specific related resource
```

---

## CRUD Operations

### List Collection

**Request:**
```http
GET /api/users
```

**Query Parameters:**
- Filtering (see Query Parameters section)
- Sorting: `?sort=field` or `?sort=-field`
- Pagination: `?limit=N&offset=M`
- Field selection: `?fields=id,email,name`

**Response:**
```json
{
  "data": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "email": "alice@example.com",
      "name": "Alice",
      "age": 30,
      "created_at": "2024-10-01T10:30:00Z"
    }
  ],
  "meta": {
    "total": 1234,
    "limit": 50,
    "offset": 0,
    "count": 50
  }
}
```

**Status Codes:**
- `200 OK`: Success
- `400 Bad Request`: Invalid query parameters
- `500 Internal Server Error`: Server error

---

### Get Single Resource

**Request:**
```http
GET /api/users/550e8400-e29b-41d4-a716-446655440000
```

**Query Parameters:**
- `?include=posts,comments`: Include relations
- `?fields=id,email`: Select specific fields
- `?compute=fullName`: Request computed fields

**Response:**
```json
{
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "alice@example.com",
    "name": "Alice",
    "age": 30,
    "created_at": "2024-10-01T10:30:00Z",
    "_computed": {
      "fullName": "Alice Smith"
    },
    "_relations": {
      "posts": [
        {
          "id": "...",
          "title": "First Post"
        }
      ]
    }
  }
}
```

**Status Codes:**
- `200 OK`: Success
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Server error

---

### Create Resource

**Request:**
```http
POST /api/users
Content-Type: application/json

{
  "email": "bob@example.com",
  "name": "Bob",
  "age": 25
}
```

**Validation:**
- Required fields enforced
- Type checking
- Constraint validation (unique, pattern, min/max, etc.)
- Foreign key validation

**Response:**
```json
{
  "data": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "email": "bob@example.com",
    "name": "Bob",
    "age": 25,
    "created_at": "2024-10-11T15:45:00Z"
  }
}
```

**Status Codes:**
- `201 Created`: Resource created
- `400 Bad Request`: Validation error
- `409 Conflict`: Unique constraint violation
- `500 Internal Server Error`: Server error

**Error Response (400):**
```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Validation failed",
    "details": [
      {
        "field": "email",
        "message": "Invalid email format"
      },
      {
        "field": "age",
        "message": "Must be between 0 and 150"
      }
    ]
  }
}
```

---

### Update Resource (Full)

**Request:**
```http
PUT /api/users/660e8400-e29b-41d4-a716-446655440000
Content-Type: application/json

{
  "email": "bob.updated@example.com",
  "name": "Robert",
  "age": 26
}
```

**Behavior:**
- Replaces entire resource
- All fields must be provided (except auto-generated)
- Validates like POST

**Response:**
```json
{
  "data": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "email": "bob.updated@example.com",
    "name": "Robert",
    "age": 26,
    "created_at": "2024-10-11T15:45:00Z",
    "updated_at": "2024-10-11T16:00:00Z"
  }
}
```

**Status Codes:**
- `200 OK`: Updated
- `404 Not Found`: Resource not found
- `400 Bad Request`: Validation error
- `409 Conflict`: Constraint violation

---

### Update Resource (Partial)

**Request:**
```http
PATCH /api/users/660e8400-e29b-41d4-a716-446655440000
Content-Type: application/json

{
  "age": 27
}
```

**Behavior:**
- Updates only provided fields
- Other fields unchanged
- Validates provided fields

**Response:**
```json
{
  "data": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "email": "bob.updated@example.com",
    "name": "Robert",
    "age": 27,
    "created_at": "2024-10-11T15:45:00Z",
    "updated_at": "2024-10-11T16:15:00Z"
  }
}
```

**Status Codes:**
- `200 OK`: Updated
- `404 Not Found`: Resource not found
- `400 Bad Request`: Validation error

---

### Delete Resource

**Request:**
```http
DELETE /api/users/660e8400-e29b-41d4-a716-446655440000
```

**Behavior:**
- Hard delete by default
- Soft delete if `@soft_delete` directive
- Cascade delete based on relation directives

**Response:**
```json
{
  "data": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "deleted": true
  }
}
```

**Status Codes:**
- `200 OK`: Deleted (or `204 No Content`)
- `404 Not Found`: Resource not found
- `409 Conflict`: Cannot delete (foreign key constraint)
- `500 Internal Server Error`: Server error

---

## Query Parameters

### Filtering

Query parameters are automatically generated based on field types.

#### Numeric Fields

```
?age=25              # Exact match
?age>25              # Greater than
?age>=25             # Greater than or equal
?age<65              # Less than
?age<=65             # Less than or equal
?age=25,30,35        # IN (any of these values)
?age>25&age<65       # Range (AND)
```

#### String Fields

```
?email=alice@example.com     # Exact match
?name~Alice                   # Contains (case-insensitive)
?name^Alice                   # Starts with
?name$Smith                   # Ends with
?name=Alice,Bob               # IN (any of these)
```

#### Boolean Fields

```
?verified=true
?verified=false
```

#### Date/Timestamp Fields

```
?created_at=2024-10-11                    # Exact date
?created_at>2024-10-01                    # After date
?created_at<2024-10-31                    # Before date
?created_at>2024-10-01&created_at<2024-10-31  # Range
```

#### UUID Fields

```
?id=550e8400-e29b-41d4-a716-446655440000
```

#### Null Checks

```
?bio=null            # IS NULL
?bio!=null           # IS NOT NULL
```

### Relation Filtering

```
# Filter by related field
?user.email=alice@example.com
?user.id=550e8400-e29b-41d4-a716-446655440000

# Filter by relation existence
?posts!=null         # Has posts
?posts=null          # Has no posts

# Nested filtering
?user.profile.bio~developer
```

### Logical Operators

```
# AND (default)
?age>25&verified=true

# OR (comma in same parameter)
?status=published,draft

# Complex (mix AND and OR)
?age>25&status=published,draft&verified=true
```

### Sorting

```
?sort=age                    # Ascending
?sort=-age                   # Descending
?sort=age,-created_at        # Multi-field
?sort=user.name              # By relation field
```

### Pagination

```
?limit=50                    # Limit results (default: 50, max: 1000)
?offset=100                  # Skip N results
?page=3&per_page=50          # Page-based (alternative)
```

### Field Selection

```
?fields=id,email,name        # Only these fields
?exclude=password_hash       # All except these
```

### Include Relations

```
?include=posts               # Include posts relation
?include=posts,comments      # Multiple relations
?include=posts.user          # Nested relations
```

### Computed Fields

```
?compute=fullName            # Request computed field
?compute=fullName,postCount  # Multiple computed fields
?compute=*                   # All computed fields
```

### Aggregations

```
?aggregate=count             # Count results
?aggregate=avg(age)          # Average
?aggregate=sum(views)        # Sum
?aggregate=min(price)        # Minimum
?aggregate=max(price)        # Maximum
?group_by=category           # Group by field
```

### Full-Text Search

For fields with `@fulltext` directive:

```
?search=machine learning     # Full-text search
?search_field=content        # Specify field
```

---

## Relationship Routes

### Get Related Resources

**Request:**
```http
GET /api/users/550e8400-e29b-41d4-a716-446655440000/posts
```

**Query Parameters:**
- Same filtering/sorting as collection endpoints

**Response:**
```json
{
  "data": [
    {
      "id": "...",
      "title": "First Post",
      "content": "...",
      "user_id": "550e8400-e29b-41d4-a716-446655440000"
    }
  ],
  "meta": {
    "total": 10,
    "limit": 50,
    "offset": 0
  }
}
```

### Add Relation (Many-to-Many)

**Request:**
```http
POST /api/posts/abc/tags
Content-Type: application/json

{
  "tag_ids": ["xyz", "def"]
}
```

**Response:**
```json
{
  "data": {
    "added": 2
  }
}
```

### Remove Relation (Many-to-Many)

**Request:**
```http
DELETE /api/posts/abc/tags/xyz
```

**Response:**
```json
{
  "data": {
    "removed": true
  }
}
```

---

## Batch Operations

### Batch Create

**Request:**
```http
POST /api/users/batch
Content-Type: application/json

{
  "data": [
    {"email": "user1@example.com", "name": "User 1"},
    {"email": "user2@example.com", "name": "User 2"}
  ]
}
```

**Response:**
```json
{
  "data": [
    {"id": "...", "email": "user1@example.com", ...},
    {"id": "...", "email": "user2@example.com", ...}
  ],
  "meta": {
    "created": 2,
    "failed": 0
  }
}
```

### Batch Update

**Request:**
```http
PATCH /api/users/batch
Content-Type: application/json

{
  "ids": ["id1", "id2", "id3"],
  "data": {
    "verified": true
  }
}
```

**Response:**
```json
{
  "data": {
    "updated": 3
  }
}
```

### Batch Delete

**Request:**
```http
DELETE /api/users/batch?age<18
```

**Response:**
```json
{
  "data": {
    "deleted": 42
  }
}
```

---

## Computed Field RPC

When computed fields cannot be calculated client-side:

**Request:**
```http
POST /api/compute/User.creditScore
Content-Type: application/json

{
  "income": 75000,
  "debt": 15000,
  "credit_history": [...]
}
```

**Response:**
```json
{
  "data": {
    "creditScore": 720
  }
}
```

---

## WebSocket API (Future)

For real-time updates:

```javascript
const ws = new WebSocket('ws://localhost:3000/api/ws');

// Subscribe to model changes
ws.send(JSON.stringify({
  action: 'subscribe',
  model: 'User',
  filters: { age: '>25' }
}));

// Receive updates
ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  // { action: 'insert', model: 'User', data: {...} }
};
```

---

## Authentication & Authorization (Plugin)

SinkDB doesn't enforce auth by default, but provides hooks:

```rust
// Custom middleware
server.add_middleware(|req, next| async {
    let token = req.headers().get("Authorization")?;
    let user = verify_token(token)?;
    req.extensions_mut().insert(user);
    next(req).await
});

// Access in generated handlers
fn get_users(req: Request) -> Response {
    let user = req.extensions().get::<User>().unwrap();
    if !user.is_admin() {
        return Response::forbidden();
    }
    // ...
}
```

---

## Rate Limiting

Configured in `sinkdb.toml`:

```toml
[api]
rate_limit = 1000  # Requests per minute per IP
rate_limit_burst = 100
```

**Response Headers:**
```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1697123456
```

**Status Code:**
- `429 Too Many Requests`

---

## CORS

Configured in `sinkdb.toml`:

```toml
[api]
cors_origins = ["http://localhost:5173", "https://app.example.com"]
cors_methods = ["GET", "POST", "PUT", "PATCH", "DELETE"]
cors_headers = ["Content-Type", "Authorization"]
```

---

## Caching

### ETags

```http
GET /api/users/123
ETag: "33a64df551425fcc55e4d42a148795d9f25f89d4"
```

Client sends:
```http
GET /api/users/123
If-None-Match: "33a64df551425fcc55e4d42a148795d9f25f89d4"
```

Response:
```http
304 Not Modified
```

### Cache-Control

```http
Cache-Control: public, max-age=60
```

---

## OpenAPI Specification

Auto-generated at `/api/docs/openapi.yaml` and `/api/docs/openapi.json`.

### Example

```yaml
openapi: 3.0.0
info:
  title: My App API
  version: 1.0.0
  description: Auto-generated API from SinkDB schema

servers:
  - url: http://localhost:3000/api
    description: Development server

paths:
  /users:
    get:
      summary: List users
      operationId: listUsers
      tags: [Users]
      parameters:
        - name: age
          in: query
          schema:
            type: integer
        - name: email
          in: query
          schema:
            type: string
            format: email
        - name: sort
          in: query
          schema:
            type: string
            enum: [id, email, age, -id, -email, -age]
        - name: limit
          in: query
          schema:
            type: integer
            minimum: 1
            maximum: 1000
            default: 50
        - name: offset
          in: query
          schema:
            type: integer
            minimum: 0
            default: 0
      responses:
        '200':
          description: Success
          content:
            application/json:
              schema:
                type: object
                properties:
                  data:
                    type: array
                    items:
                      $ref: '#/components/schemas/User'
                  meta:
                    $ref: '#/components/schemas/PaginationMeta'
        '400':
          $ref: '#/components/responses/BadRequest'
        '500':
          $ref: '#/components/responses/InternalError'

  /users/{id}:
    get:
      summary: Get user by ID
      operationId: getUser
      tags: [Users]
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
            format: uuid
      responses:
        '200':
          description: Success
          content:
            application/json:
              schema:
                type: object
                properties:
                  data:
                    $ref: '#/components/schemas/User'
        '404':
          $ref: '#/components/responses/NotFound'

components:
  schemas:
    User:
      type: object
      required:
        - id
        - email
        - name
        - age
      properties:
        id:
          type: string
          format: uuid
          readOnly: true
        email:
          type: string
          format: email
          minLength: 1
          maxLength: 255
        name:
          type: string
          minLength: 1
        age:
          type: integer
          minimum: 0
          maximum: 150
        bio:
          type: string
          nullable: true
        created_at:
          type: string
          format: date-time
          readOnly: true
        updated_at:
          type: string
          format: date-time
          readOnly: true

    PaginationMeta:
      type: object
      properties:
        total:
          type: integer
        limit:
          type: integer
        offset:
          type: integer
        count:
          type: integer

  responses:
    BadRequest:
      description: Bad request
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
    
    NotFound:
      description: Resource not found
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
    
    InternalError:
      description: Internal server error
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
```

---

## Custom Endpoints

For operations not covered by generated CRUD:

```rust
// In your application code
use sinkdb::api::Server;

let mut server = Server::from_schema()?;

// Add custom endpoint
server.add_route("/api/custom/analytics", |req| async {
    // Custom logic
    let data = compute_analytics().await?;
    Response::json(data)
});

server.run().await?;
```

---

## Error Handling

### Standard Error Format

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable message",
    "details": [
      {
        "field": "email",
        "message": "Invalid email format"
      }
    ],
    "trace_id": "abc123"  // For debugging
  }
}
```

### Error Codes

```
VALIDATION_ERROR       # 400 - Request validation failed
NOT_FOUND             # 404 - Resource not found
CONFLICT              # 409 - Unique constraint violation
UNAUTHORIZED          # 401 - Authentication required
FORBIDDEN             # 403 - Insufficient permissions
RATE_LIMITED          # 429 - Too many requests
INTERNAL_ERROR        # 500 - Server error
```

---

## Performance Considerations

### Generated Query Optimization

The transpiler analyzes queries and generates optimized code:

```rust
// User query: GET /api/users?age>25&email~@gmail

// Generated code:
fn query_users(filters: Filters) -> Vec<User> {
    // Step 1: Filter age column (vectorized)
    let age_matches = self.user_ages
        .iter()
        .enumerate()
        .filter(|(_, &age)| age > 25)
        .map(|(i, _)| i)
        .collect::<BitVec>();
    
    // Step 2: Filter email (only for age matches)
    let matches = age_matches.iter_ones()
        .filter(|&i| self.user_emails[i].contains("@gmail"))
        .collect();
    
    // Step 3: Materialize users
    self.load_users(&matches)
}
```

### Pagination

Large result sets are automatically paginated:

```
Default limit: 50
Max limit: 1000
```

### Caching

Repeated queries with same parameters are cached (configurable).

---

## API Versioning (Future)

```
/api/v1/users
/api/v2/users
```

Generated from schema versions.

---

**Document Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Specification
