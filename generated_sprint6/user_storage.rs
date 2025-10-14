// Generated code - do not edit manually

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: uuid::Uuid,
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
        let id = Uuid::new_v4();
        let record = User {
            id,
            email,
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

    pub fn update(&mut self, id: uuid::Uuid, email: String) -> Result<User, String> {
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

