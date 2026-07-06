//! ForgeDB CRUD API
//!
//! Generic CRUD operation handlers for database models with type-safe operations
//! and HTTP integration.
//!
//! # Overview
//!
//! This crate provides generic CRUD (Create, Read, Update, Delete) operation handlers
//! for ForgeDB models, offering:
//!
//! - **Generic operations** - Type-safe CRUD operations for any model
//! - **HTTP handlers** - Ready-to-use HTTP endpoint handlers
//! - **List operations** - Paginated list endpoints with filtering
//! - **Error handling** - Comprehensive error types and responses
//! - **Type safety** - Full type safety for model operations
//!
//! # Architecture
//!
//! The CRUD API is built around two main concepts:
//!
//! 1. **CrudOperations Trait** - Generic trait for storage implementations
//! 2. **CrudHandlers** - HTTP handlers that use CrudOperations
//!
//! ## Operation Flow
//!
//! ```text
//! HTTP Request
//!     ↓
//! CrudHandlers (parse, validate)
//!     ↓
//! CrudOperations (execute on storage)
//!     ↓
//! Storage Layer
//!     ↓
//! HTTP Response (JSON)
//! ```
//!
//! # Examples
//!
//! ## Implementing CrudOperations
//!
//! ```rust
//! use forgedb_crud_api::{CrudOperations, CrudResult};
//! use uuid::Uuid;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct User {
//!     id: Uuid,
//!     email: String,
//!     name: String,
//! }
//!
//! #[derive(Debug, Deserialize)]
//! struct CreateUser {
//!     email: String,
//!     name: String,
//! }
//!
//! #[derive(Debug, Deserialize)]
//! struct UpdateUser {
//!     email: Option<String>,
//!     name: Option<String>,
//! }
//!
//! struct UserStorage {
//!     users: Vec<User>,
//! }
//!
//! impl CrudOperations for UserStorage {
//!     type Model = User;
//!     type CreateInput = CreateUser;
//!     type UpdateInput = UpdateUser;
//!
//!     fn list(&self) -> CrudResult<Vec<Self::Model>> {
//!         Ok(self.users.clone())
//!     }
//!
//!     fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
//!         Ok(self.users.iter().find(|u| u.id == *id).cloned())
//!     }
//!
//!     fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
//!         let user = User {
//!             id: Uuid::new_v4(),
//!             email: input.email,
//!             name: input.name,
//!         };
//!         self.users.push(user.clone());
//!         Ok(user)
//!     }
//!
//!     fn update(&mut self, id: &Uuid, input: Self::UpdateInput) -> CrudResult<Option<Self::Model>> {
//!         if let Some(user) = self.users.iter_mut().find(|u| u.id == *id) {
//!             if let Some(email) = input.email {
//!                 user.email = email;
//!             }
//!             if let Some(name) = input.name {
//!                 user.name = name;
//!             }
//!             Ok(Some(user.clone()))
//!         } else {
//!             Ok(None)
//!         }
//!     }
//!
//!     fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
//!         let before_len = self.users.len();
//!         self.users.retain(|u| u.id != *id);
//!         Ok(self.users.len() < before_len)
//!     }
//! }
//! ```
//!
//! ## Using CRUD Handlers
//!
//! [`CrudHandlers`] wraps a storage that implements [`CrudOperations`] and
//! provides typed list / get / create / update / delete methods.  HTTP wiring
//! (axum routes, etc.) is intentionally left to the caller so the crate stays
//! free of an axum dependency.
//!
//! ```rust
//! use forgedb_crud_api::{CrudHandlers, CrudOperations, CrudResult};
//! use uuid::Uuid;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct User { id: Uuid, email: String }
//!
//! #[derive(Debug, Deserialize)]
//! struct CreateUser { email: String }
//!
//! #[derive(Debug, Deserialize)]
//! struct UpdateUser { email: Option<String> }
//!
//! struct UserStorage { users: Vec<User> }
//!
//! impl CrudOperations for UserStorage {
//!     type Model = User;
//!     type CreateInput = CreateUser;
//!     type UpdateInput = UpdateUser;
//!     fn list(&self) -> CrudResult<Vec<Self::Model>> { Ok(self.users.clone()) }
//!     fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
//!         Ok(self.users.iter().find(|u| u.id == *id).cloned())
//!     }
//!     fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
//!         let u = User { id: Uuid::new_v4(), email: input.email };
//!         self.users.push(u.clone());
//!         Ok(u)
//!     }
//!     fn update(&mut self, id: &Uuid, input: Self::UpdateInput) -> CrudResult<Option<Self::Model>> {
//!         if let Some(u) = self.users.iter_mut().find(|u| u.id == *id) {
//!             if let Some(email) = input.email { u.email = email; }
//!             Ok(Some(u.clone()))
//!         } else { Ok(None) }
//!     }
//!     fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
//!         let before = self.users.len();
//!         self.users.retain(|u| u.id != *id);
//!         Ok(self.users.len() < before)
//!     }
//! }
//!
//! let mut handlers = CrudHandlers::new(UserStorage { users: vec![] });
//!
//! // Create a record, then list it back.
//! let user = handlers.create(CreateUser { email: "alice@example.com".to_string() }).unwrap();
//! assert_eq!(user.email, "alice@example.com");
//!
//! let users = handlers.list().unwrap();
//! assert_eq!(users.len(), 1);
//! ```
//!
//! ## List with Pagination
//!
//! The real fields are `data`, `total`, `limit`, and `offset`
//! (not `page` / `per_page`).  Use the constructors to set them correctly.
//!
//! ```rust
//! use forgedb_crud_api::ListResponse;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct User { id: String, email: String }
//!
//! let users = vec![
//!     User { id: "1".to_string(), email: "user1@example.com".to_string() },
//!     User { id: "2".to_string(), email: "user2@example.com".to_string() },
//! ];
//!
//! // with_pagination(data, total, limit, offset)
//! let response = ListResponse::with_pagination(users, 2, 10, 0);
//! assert_eq!(response.total, 2);
//! assert_eq!(response.limit, Some(10));
//! assert_eq!(response.offset, Some(0));
//!
//! let json = serde_json::to_string(&response).unwrap();
//! assert!(json.contains("user1@example.com"));
//! ```
//!
//! # Public API
//!
//! ## Core Traits
//!
//! - [`CrudOperations`] - Trait for implementing CRUD operations on storage
//!
//! ## HTTP Handlers
//!
//! - [`CrudHandlers`] - HTTP handler struct with methods for each operation
//!
//! ## Response Types
//!
//! - [`ListResponse<T>`] - Paginated list response with metadata
//! - [`CrudResult<T>`] - Result type for CRUD operations
//!
//! ## Error Types
//!
//! - [`CrudError`] - Error type for CRUD operations
//!
//! # CRUD Operations
//!
//! ## List (GET /resource)
//!
//! Returns all records (excluding tombstoned):
//! - Response: `ListResponse<Model>` with pagination info
//! - Status: 200 OK
//!
//! ## Get (GET /resource/:id)
//!
//! Returns a single record by ID:
//! - Response: `Model` if found
//! - Status: 200 OK or 404 Not Found
//!
//! ## Create (POST /resource)
//!
//! Creates a new record:
//! - Request: `CreateInput` (JSON body)
//! - Response: Created `Model`
//! - Status: 201 Created
//!
//! ## Update (PUT /resource/:id)
//!
//! Updates an existing record:
//! - Request: `UpdateInput` (JSON body)
//! - Response: Updated `Model` if found
//! - Status: 200 OK or 404 Not Found
//!
//! ## Delete (DELETE /resource/:id)
//!
//! Marks a record as deleted (tombstone):
//! - Response: Empty
//! - Status: 204 No Content or 404 Not Found
//!
//! ## Count (GET /resource/count)
//!
//! Returns count of non-tombstoned records:
//! - Response: `{ "count": number }`
//! - Status: 200 OK
//!
//! # Error Handling
//!
//! CRUD operations return errors in these cases:
//!
//! - **NotFound**: Resource doesn't exist
//! - **ValidationError**: Input validation failed
//! - **DatabaseError**: Storage operation failed
//! - **Conflict**: Uniqueness constraint violation
//!
//! # Related Crates
//!
//! - [`forgedb-http-server`](../forgedb_http_server) - HTTP server infrastructure
//! - [`forgedb-storage`](../forgedb_storage) - Storage layer implementation
//! - [`forgedb-query-params`](../forgedb_query_params) - Query parameter parsing
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [SPRINT8_CRUD_API.md](../../archive/sprint-summaries/SPRINT8_CRUD_API.md) - CRUD API implementation

mod handlers;
mod operations;

pub use handlers::{CrudError, CrudHandlers, CrudResult, ListResponse};
pub use operations::{
    CreateOperation, DeleteOperation, GetOperation, ListOperation, UpdateOperation,
};

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use uuid::Uuid;

/// Generic CRUD operations trait that storage types can implement
pub trait CrudOperations {
    /// The type of the model (e.g., User, Post)
    type Model: Clone + Debug + Serialize + for<'de> Deserialize<'de>;

    /// The type used for creating (may have fewer fields than Model)
    type CreateInput: Debug + for<'de> Deserialize<'de>;

    /// The type used for updating (fields are optional)
    type UpdateInput: Debug + for<'de> Deserialize<'de>;

    /// List all records (excluding tombstoned records)
    fn list(&self) -> CrudResult<Vec<Self::Model>>;

    /// Get a record by ID
    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>>;

    /// Create a new record
    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model>;

    /// Update an existing record
    fn update(&mut self, id: &Uuid, input: Self::UpdateInput) -> CrudResult<Option<Self::Model>>;

    /// Delete a record (marks as tombstoned)
    fn delete(&mut self, id: &Uuid) -> CrudResult<bool>;

    /// Get the count of non-tombstoned records
    fn count(&self) -> CrudResult<usize> {
        Ok(self.list()?.len())
    }
}

