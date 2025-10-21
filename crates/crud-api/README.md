# ForgeDB CRUD API

Generic CRUD operation handlers for ForgeDB database models, providing a trait-based abstraction layer for Create, Read, Update, and Delete operations.

## Overview

The `forgedb-crud-api` crate provides a flexible, type-safe CRUD API that can be implemented by any storage backend. It offers:

- Generic trait-based design that works with any model type
- Standardized error handling across operations
- Pagination support with `ListResponse`
- Clear separation between create/update input types and model types
- Immutable and mutable operation handlers

## Features

### CRUD Operations
- **Create** - Add new records with validated input
- **Read** - Retrieve individual records by ID or list all records
- **Update** - Modify existing records with partial updates
- **Delete** - Remove records (supports soft deletion/tombstoning)
- **List** - Fetch all records with pagination support
- **Count** - Get total count of non-deleted records

### Error Handling
Comprehensive error types for common CRUD scenarios:
- `NotFound` - Resource doesn't exist
- `ValidationError` - Invalid input data
- `Conflict` - Constraint violations (e.g., unique keys)
- `Internal` - Storage or system errors

### Pagination
`ListResponse` structure provides standardized list responses with:
- Data array
- Total count
- Optional limit and offset for pagination

## Usage Examples

### Basic Implementation

Implement the `CrudOperations` trait for your storage type:

```rust
use forgedb_crud_api::{CrudOperations, CrudResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: Uuid,
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct UpdateUser {
    name: Option<String>,
    email: Option<String>,
}

struct UserStorage {
    users: Vec<User>,
}

impl CrudOperations for UserStorage {
    type Model = User;
    type CreateInput = CreateUser;
    type UpdateInput = UpdateUser;

    fn list(&self) -> CrudResult<Vec<Self::Model>> {
        Ok(self.users.clone())
    }

    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
        Ok(self.users.iter().find(|u| &u.id == id).cloned())
    }

    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
        let user = User {
            id: Uuid::new_v4(),
            name: input.name,
            email: input.email,
        };
        self.users.push(user.clone());
        Ok(user)
    }

    fn update(
        &mut self,
        id: &Uuid,
        input: Self::UpdateInput,
    ) -> CrudResult<Option<Self::Model>> {
        if let Some(user) = self.users.iter_mut().find(|u| &u.id == id) {
            if let Some(name) = input.name {
                user.name = name;
            }
            if let Some(email) = input.email {
                user.email = email;
            }
            Ok(Some(user.clone()))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
        let initial_len = self.users.len();
        self.users.retain(|u| &u.id != id);
        Ok(self.users.len() < initial_len)
    }
}
```

### Using CrudHandlers

Wrap your storage with `CrudHandlers` for convenient access:

```rust
use forgedb_crud_api::CrudHandlers;

fn main() {
    let storage = UserStorage { users: vec![] };
    let mut handlers = CrudHandlers::new(storage);

    // Create a user
    let user = handlers.create(CreateUser {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    }).unwrap();

    println!("Created user: {} with ID {}", user.name, user.id);

    // Get the user
    let retrieved = handlers.get(&user.id).unwrap();
    println!("Retrieved: {}", retrieved.name);

    // Update the user
    let updated = handlers.update(&user.id, UpdateUser {
        name: Some("Alice Smith".to_string()),
        email: None,
    }).unwrap();
    println!("Updated name to: {}", updated.name);

    // List all users
    let all_users = handlers.list().unwrap();
    println!("Total users: {}", all_users.len());

    // Delete the user
    handlers.delete(&user.id).unwrap();
    println!("User deleted");
}
```

### Error Handling Patterns

```rust
use forgedb_crud_api::{CrudError, CrudHandlers};
use uuid::Uuid;

fn handle_user_operations(handlers: &mut CrudHandlers<UserStorage>) {
    let user_id = Uuid::new_v4();
    
    match handlers.get(&user_id) {
        Ok(user) => println!("Found user: {}", user.name),
        Err(CrudError::NotFound(msg)) => println!("User not found: {}", msg),
        Err(CrudError::ValidationError(msg)) => println!("Invalid data: {}", msg),
        Err(CrudError::Conflict(msg)) => println!("Conflict: {}", msg),
        Err(CrudError::Internal(msg)) => println!("System error: {}", msg),
    }
}
```

### Using ListResponse

```rust
use forgedb_crud_api::ListResponse;

fn list_users_with_pagination(handlers: &CrudHandlers<UserStorage>) {
    let users = handlers.list().unwrap();
    
    // Simple response without pagination
    let response = ListResponse::new(users.clone());
    println!("Total: {}", response.total);
    
    // With pagination metadata
    let page_size = 10;
    let offset = 0;
    let page_data = users.iter().skip(offset).take(page_size).cloned().collect();
    let response = ListResponse::with_pagination(
        page_data,
        users.len(),
        page_size,
        offset
    );
    
    println!("Page data: {} items", response.data.len());
    println!("Total: {}, Limit: {:?}, Offset: {:?}", 
        response.total, response.limit, response.offset);
}
```

### Individual Operations

For fine-grained control, use individual operation structs:

```rust
use forgedb_crud_api::{ListOperation, GetOperation, CreateOperation};

fn use_individual_operations() {
    let mut storage = UserStorage { users: vec![] };
    
    // List operation
    let users = ListOperation::execute(&storage).unwrap();
    
    // Create operation
    let user = CreateOperation::execute(&mut storage, CreateUser {
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
    }).unwrap();
    
    // Get operation
    let retrieved = GetOperation::execute(&storage, &user.id).unwrap();
}
```

## API

### Core Traits

#### `CrudOperations`

The main trait that storage implementations must implement:

```rust
pub trait CrudOperations {
    type Model: Clone + Debug + Serialize + for<'de> Deserialize<'de>;
    type CreateInput: Debug + for<'de> Deserialize<'de>;
    type UpdateInput: Debug + for<'de> Deserialize<'de>;

    fn list(&self) -> CrudResult<Vec<Self::Model>>;
    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>>;
    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model>;
    fn update(&mut self, id: &Uuid, input: Self::UpdateInput) 
        -> CrudResult<Option<Self::Model>>;
    fn delete(&mut self, id: &Uuid) -> CrudResult<bool>;
    fn count(&self) -> CrudResult<usize>;
}
```

### Handlers

#### `CrudHandlers<T>`

Wrapper providing convenient CRUD operations:

- `new(storage: T)` - Create handlers with storage backend
- `list()` - Get all records
- `get(id)` - Get record by ID (returns error if not found)
- `create(input)` - Create new record
- `update(id, input)` - Update existing record (returns error if not found)
- `delete(id)` - Delete record (returns error if not found)
- `count()` - Get total count
- `storage()` - Get immutable reference to storage
- `storage_mut()` - Get mutable reference to storage

### Error Types

#### `CrudError`

```rust
pub enum CrudError {
    NotFound(String),
    ValidationError(String),
    Conflict(String),
    Internal(String),
}
```

All errors implement `std::error::Error` and `Display`.

### Response Types

#### `ListResponse<T>`

Standardized list response format:

```rust
pub struct ListResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: usize,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
```

**Constructors:**
- `new(data)` - Create from data (no pagination)
- `with_pagination(data, total, limit, offset)` - With pagination metadata
- `with_total(data, total)` - With explicit total (when different from data.len())

### Operation Structs

Individual operation types for granular control:

- `ListOperation::execute(storage)` - List all records
- `GetOperation::execute(storage, id)` - Get single record
- `CreateOperation::execute(storage, input)` - Create record
- `UpdateOperation::execute(storage, id, input)` - Update record
- `DeleteOperation::execute(storage, id)` - Delete record

## Integration

### With forgedb-storage

The crud-api is designed to work seamlessly with `forgedb-storage`:

```rust
use forgedb_storage::Storage;
use forgedb_crud_api::{CrudOperations, CrudHandlers, CrudResult};

// Implement CrudOperations for your storage wrapper
struct MyModelStorage {
    storage: Storage,
}

impl CrudOperations for MyModelStorage {
    type Model = MyModel;
    type CreateInput = CreateMyModel;
    type UpdateInput = UpdateMyModel;
    
    // Implement methods using forgedb-storage APIs
    fn list(&self) -> CrudResult<Vec<Self::Model>> {
        // Use storage.list() or similar
        todo!()
    }
    
    // ... other methods
}
```

### With forgedb-http-server

Use `CrudHandlers` in HTTP endpoints:

```rust
// Example HTTP handler (pseudo-code)
async fn create_user_endpoint(
    handlers: &mut CrudHandlers<UserStorage>,
    input: CreateUser,
) -> Result<Response, Error> {
    match handlers.create(input) {
        Ok(user) => Ok(json_response(user, 201)),
        Err(CrudError::ValidationError(msg)) => Ok(error_response(msg, 400)),
        Err(CrudError::Conflict(msg)) => Ok(error_response(msg, 409)),
        Err(err) => Ok(error_response(err.to_string(), 500)),
    }
}
```

Map `CrudError` to HTTP status codes:
- `NotFound` → 404
- `ValidationError` → 400
- `Conflict` → 409
- `Internal` → 500

## Testing

Run the test suite:

```bash
# Run all tests
cargo test -p forgedb-crud-api

# Run with output
cargo test -p forgedb-crud-api -- --nocapture

# Run specific test file
cargo test -p forgedb-crud-api --test handlers_tests
```

The test suite includes:
- Handler operation tests (create, read, update, delete, list)
- Error handling tests (not found scenarios)
- List response formatting tests
- Individual operation tests
- Count operation tests

## Design Principles

1. **Type Safety** - Generic traits ensure compile-time type checking
2. **Flexibility** - Works with any storage backend that implements `CrudOperations`
3. **Clear Separation** - Distinct types for create/update inputs vs models
4. **Ergonomics** - `CrudHandlers` provides convenient error conversion
5. **Consistency** - Standardized response formats and error types

## Dependencies

- `forgedb-storage` - Storage engine integration
- `uuid` - Unique identifiers
- `serde` - Serialization/deserialization
- `serde_json` - JSON support

## License

Part of the ForgeDB project - MIT OR Apache-2.0
