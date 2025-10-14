// Generated code - do not edit manually
// This file demonstrates what generated code looks like

use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn main() {
    println!("=== Generated Code Example ===\n");
    println!("This example shows the structure of generated database code.");
    println!("In a real application, this would be generated from a schema.sink file.\n");

    // Create a new database and test basic operations
    let mut db = Database::new();

    println!("Inserting a test user...");
    match db.user.insert(
        "test@example.com".to_string(),
        25,
        100.50,
        true,
        42
    ) {
        Ok(user) => {
            println!("✓ User created successfully!");
            println!("  ID: {}", user.id);
            println!("  Email: {}", user.email);
            println!("  Age: {}", user.age);
        }
        Err(e) => println!("✗ Error: {}", e),
    }

    println!("\nThis demonstrates the generated API structure.");
}

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: uuid::Uuid,
    pub email: String,
    pub age: u32,
    pub balance: f64,
    pub active: bool,
    pub score: i32,
    pub created_at: i64,
}

pub struct UserStorage {
    records: Vec<User>,
    next_id: u64,
    tombstones: Vec<bool>,
    email_index: std::collections::HashMap<String, usize>,
}

impl UserStorage {
    pub fn new() -> Self {
        UserStorage {
            records: Vec::new(),
            next_id: 1,
            tombstones: Vec::new(),
            email_index: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, email: String, age: u32, balance: f64, active: bool, score: i32) -> Result<User, String> {
        if self.email_index.contains_key(&email) {
            return Err("Unique constraint violation: email already exists".to_string());
        }
        let id = Uuid::new_v4();
        let created_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let record = User {
            id,
            email,
            age,
            balance,
            active,
            score,
            created_at,
        };

        let row_index = self.records.len();
        self.email_index.insert(record.email.clone(), row_index);
        self.records.push(record.clone());
        self.tombstones.push(false);
        Ok(record)
    }

    pub fn get(&self, id: uuid::Uuid) -> Option<User> {
        self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(_, r)| r.clone())
    }

    pub fn find_by_email(&self, email: String) -> Vec<User> {
        if let Some(&idx) = self.email_index.get(&email) {
            if !self.tombstones[idx] {
                return vec![self.records[idx].clone()];
            }
        }
        Vec::new()
    }

    pub fn list(&self) -> Vec<User> {
        self.records.iter().enumerate()
            .filter(|(i, _)| !self.tombstones[*i])
            .map(|(_, r)| r.clone())
            .collect()
    }

    pub fn update(&mut self, id: uuid::Uuid, email: String, age: u32, balance: f64, active: bool, score: i32) -> Result<User, String> {
        let idx = self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(i, _)| i)
            .ok_or_else(|| "Record not found".to_string())?;

        self.email_index.remove(&self.records[idx].email);
        if self.email_index.contains_key(&email) && self.records[idx].email != email {
            return Err("Unique constraint violation: email already exists".to_string());
        }
        self.records[idx] = User {
            id: self.records[idx].id.clone(),
            email,
            age,
            balance,
            active,
            score,
            created_at: self.records[idx].created_at.clone(),
        };

        self.email_index.insert(self.records[idx].email.clone(), idx);
        Ok(self.records[idx].clone())
    }

    pub fn delete(&mut self, id: uuid::Uuid) -> Result<(), String> {
        let idx = self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(i, _)| i)
            .ok_or_else(|| "Record not found".to_string())?;

        self.tombstones[idx] = true;

        self.email_index.remove(&self.records[idx].email);
        Ok(())
    }

}

pub struct Database {
    pub user: UserStorage,
}

impl Database {
    pub fn new() -> Self {
        Database {
            user: UserStorage::new(),
        }
    }

}

