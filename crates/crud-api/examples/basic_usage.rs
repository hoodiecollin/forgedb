//! Basic usage example for forgedb-crud-api
//!
//! This example demonstrates implementing CRUD operations
//! for a simple User model.

use forgedb_crud_api::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Define a User model
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: Uuid,
    name: String,
    email: String,
    age: u32,
}

// Define input types for creating/updating users
#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
    email: String,
    age: u32,
}

#[derive(Debug, Deserialize)]
struct UpdateUser {
    name: Option<String>,
    email: Option<String>,
    age: Option<u32>,
}

// Simple in-memory storage implementation
struct UserStorage {
    users: HashMap<Uuid, User>,
}

impl UserStorage {
    fn new() -> Self {
        Self {
            users: HashMap::new(),
        }
    }
}

// Implement CRUD operations for UserStorage
impl CrudOperations for UserStorage {
    type Model = User;
    type CreateInput = CreateUser;
    type UpdateInput = UpdateUser;

    fn list(&self) -> CrudResult<Vec<Self::Model>> {
        Ok(self.users.values().cloned().collect())
    }

    fn get(&self, id: &Uuid) -> CrudResult<Option<Self::Model>> {
        Ok(self.users.get(id).cloned())
    }

    fn create(&mut self, input: Self::CreateInput) -> CrudResult<Self::Model> {
        let user = User {
            id: Uuid::new_v4(),
            name: input.name,
            email: input.email,
            age: input.age,
        };
        
        self.users.insert(user.id, user.clone());
        Ok(user)
    }

    fn update(
        &mut self,
        id: &Uuid,
        input: Self::UpdateInput,
    ) -> CrudResult<Option<Self::Model>> {
        if let Some(user) = self.users.get_mut(id) {
            if let Some(name) = input.name {
                user.name = name;
            }
            if let Some(email) = input.email {
                user.email = email;
            }
            if let Some(age) = input.age {
                user.age = age;
            }
            Ok(Some(user.clone()))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, id: &Uuid) -> CrudResult<bool> {
        Ok(self.users.remove(id).is_some())
    }
}

fn main() {
    println!("=== ForgeDB CRUD API - Basic Usage ===\n");

    // Create storage instance
    let mut storage = UserStorage::new();
    println!("✓ Created user storage\n");

    // Create some users
    println!("--- Creating Users ---");
    let alice = storage
        .create(CreateUser {
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            age: 30,
        })
        .unwrap();
    println!("Created user: {} ({})", alice.name, alice.id);

    let bob = storage
        .create(CreateUser {
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
            age: 25,
        })
        .unwrap();
    println!("Created user: {} ({})", bob.name, bob.id);

    let charlie = storage
        .create(CreateUser {
            name: "Charlie".to_string(),
            email: "charlie@example.com".to_string(),
            age: 35,
        })
        .unwrap();
    println!("Created user: {} ({})\n", charlie.name, charlie.id);

    // List all users
    println!("--- Listing All Users ---");
    let users = storage.list().unwrap();
    println!("Total users: {}", users.len());
    for user in &users {
        println!("  - {} ({}, age {})", user.name, user.email, user.age);
    }
    println!();

    // Get a specific user
    println!("--- Getting Specific User ---");
    if let Some(user) = storage.get(&alice.id).unwrap() {
        println!("Found user: {:?}\n", user);
    }

    // Update a user
    println!("--- Updating User ---");
    let updated = storage
        .update(
            &bob.id,
            UpdateUser {
                name: None,
                email: Some("bob.new@example.com".to_string()),
                age: Some(26),
            },
        )
        .unwrap();
    
    if let Some(user) = updated {
        println!("Updated user: {} now has email {} and age {}\n", user.name, user.email, user.age);
    }

    // Delete a user
    println!("--- Deleting User ---");
    let deleted = storage.delete(&charlie.id).unwrap();
    println!("Deleted user '{}': {}\n", charlie.name, deleted);

    // List remaining users
    println!("--- Final User List ---");
    let remaining_users = storage.list().unwrap();
    println!("Remaining users: {}", remaining_users.len());
    for user in &remaining_users {
        println!("  - {} ({}, age {})", user.name, user.email, user.age);
    }

    // Count users
    println!("\n--- Count ---");
    let count = storage.count().unwrap();
    println!("Total non-deleted users: {}", count);

    println!("\n✓ Example completed successfully!");
}
