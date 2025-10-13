# Sprint 1 MVP Example

This example demonstrates the complete end-to-end functionality of the Sprint 1 MVP release of SinkDB.

## What's Included

### Sprint 1 Features Demonstrated

✅ **Schema Parsing**: Parse a simple User model with primitive types
✅ **Code Generation**: Generate compilable Rust code from schema
✅ **Auto-increment ID**: Automatic ID generation with `+u64`
✅ **Unique Constraints**: Enforce uniqueness with `&string`
✅ **CRUD Operations**: Insert and retrieve records
✅ **In-Memory Storage**: Vec-based storage with tombstone bitmap

## Files

```
sprint1_mvp/
├── README.md           # This file
├── schema.sink         # Schema definition
├── app.rs             # Application that parses schema and generates code
├── client.rs          # Client that uses the generated database
└── generated/         # Generated code (created when app runs)
    └── database.rs
```

## Schema

```
User {
  id: +u64          # Auto-increment ID
  email: &string    # Unique email
}
```

## Running the Example

### Step 1: Generate the Database Code

Run the application to parse the schema and generate code:

```bash
cargo run --example sprint1_mvp_app
```

This will:
1. Parse `schema.sink`
2. Generate `generated/database.rs`
3. Show what operations are available

### Step 2: Run the Client

Run the client to use the generated database:

```bash
cargo run --example sprint1_mvp_client
```

This will:
1. Create a `UserStorage` instance
2. Insert multiple users
3. Test unique constraint enforcement
4. Retrieve users by ID
5. Verify all Sprint 1 success criteria

## Expected Output

### From `app.rs`:

```
=== Sprint 1 MVP Example Application ===

Step 1: Parsing schema...
  ✓ Parsed 1 model(s)

Step 2: Generating code...
  ✓ Generated code written to examples/sprint1_mvp/generated/database.rs

Step 3: Generated Database Usage
  The generated code provides:
    - User struct with id: u64 and email: String
    - UserStorage with in-memory Vec storage
    - insert() method with auto-increment and unique constraints
    - get() method for retrieval by ID
    - Tombstone bitmap for soft deletes

=== Sprint 1 MVP Complete! ===
✓ Schema → Code → Database pipeline working
```

### From `client.rs`:

```
=== Sprint 1 MVP Client ===

✓ Initialized UserStorage

Test 1: Insert users with auto-increment ID
--------------------------------------------------
  Inserted: User { id: 1, email: "alice@example.com" }
  Inserted: User { id: 2, email: "bob@example.com" }
  Inserted: User { id: 3, email: "charlie@example.com" }
  ✓ Auto-increment working correctly

Test 2: Enforce unique email constraint
--------------------------------------------------
  Attempted to insert duplicate email: alice@example.com
  Got expected error: Email already exists
  ✓ Unique constraint enforced

Test 3: Retrieve users by ID
--------------------------------------------------
  Retrieved user 1: User { id: 1, email: "alice@example.com" }
  Retrieved user 2: User { id: 2, email: "bob@example.com" }
  Retrieved user 3: User { id: 3, email: "charlie@example.com" }
  ✓ All retrievals successful

Test 4: Non-existent ID returns None
--------------------------------------------------
  storage.get(999) = None
  ✓ Correctly handles non-existent IDs

==================================================
=== All Sprint 1 Success Criteria Met! ===
==================================================
✓ Parse simple schema
✓ Generate compilable Rust code
✓ Insert users with auto-increment ID
✓ Enforce unique email constraint
✓ Retrieve users by ID
✓ All in-memory, no crashes

🎉 Sprint 1 MVP is complete and functional!
```

## Key Success Criteria Validated

| Criterion | Status | Demonstrated By |
|-----------|--------|-----------------|
| Parse simple schema | ✅ | `app.rs` successfully parses `schema.sink` |
| Generate compilable Rust code | ✅ | `database.rs` is generated and compiles |
| Insert users with auto-increment ID | ✅ | IDs increment from 1, 2, 3... |
| Enforce unique email constraint | ✅ | Duplicate email insertion fails |
| Retrieve users by ID | ✅ | `get()` returns correct users |
| All in-memory, no crashes | ✅ | Runs without panics or errors |

## What's Next?

This MVP demonstrates the core concept. Sprint 2 will add:
- **Persistence**: Memory-mapped files for data storage
- **Expanded Types**: i32, i64, f64, bool, uuid, timestamp
- **Schema Validation**: Enforce naming conventions and detect errors

## Architecture Notes

### Generated Code Structure

The `database.rs` file contains:

```rust
// Data model
pub struct User {
    pub id: u64,
    pub email: String,
}

// Storage engine
pub struct UserStorage {
    data: Vec<User>,
    tombstones: Vec<bool>,
    next_id: u64,
    email_index: std::collections::HashMap<String, u64>,
}

// Operations
impl UserStorage {
    pub fn new() -> Self { ... }
    pub fn insert(&mut self, email: String) -> Result<User, String> { ... }
    pub fn get(&self, id: u64) -> Option<User> { ... }
}
```

### Storage Model

- **Vec-based storage**: Simple in-memory vector
- **Tombstone bitmap**: Track deleted records
- **Hash index**: Fast lookups for unique fields
- **Auto-increment**: Simple counter for IDs

## Testing

The client serves as both an example and a test suite, validating:
- Successful operations (insert, retrieve)
- Error conditions (duplicate email)
- Edge cases (non-existent ID)

## Limitations (Sprint 1)

- No persistence (data lost on restart)
- Single model only
- Limited types (u64, string)
- No relations
- No query operations beyond get-by-id

These will be addressed in future sprints.
