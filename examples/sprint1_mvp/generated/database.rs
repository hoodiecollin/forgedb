// Generated code - do not edit manually

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub email: String,
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

    pub fn insert(&mut self, email: String) -> Result<User, String> {
        if self.email_index.contains_key(&email) {
            return Err("Unique constraint violation: email already exists".to_string());
        }
        let id = self.next_id;
        self.next_id += 1;
        let record = User {
            id,
            email,
        };

        self.email_index.insert(record.email.clone(), self.records.len());
        self.records.push(record.clone());
        self.tombstones.push(false);
        Ok(record)
    }

    pub fn get(&self, id: u64) -> Option<User> {
        self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(_, r)| r.clone())
    }
}

