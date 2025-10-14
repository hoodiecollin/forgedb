// Generated code - do not edit manually

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub id: uuid::Uuid,
    pub name: String,
}

pub struct TagStorage {
    records: Vec<Tag>,
    next_id: u64,
    tombstones: Vec<bool>,
    name_index: std::collections::HashMap<String, usize>,
}

impl TagStorage {
    pub fn new() -> Self {
        TagStorage {
            records: Vec::new(),
            next_id: 1,
            tombstones: Vec::new(),
            name_index: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, name: String) -> Result<Tag, String> {
        if self.name_index.contains_key(&name) {
            return Err("Unique constraint violation: name already exists".to_string());
        }
        let id = Uuid::new_v4();
        let record = Tag {
            id,
            name,
        };

        let row_index = self.records.len();
        self.name_index.insert(record.name.clone(), row_index);
        self.records.push(record.clone());
        self.tombstones.push(false);
        Ok(record)
    }

    pub fn get(&self, id: uuid::Uuid) -> Option<Tag> {
        self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(_, r)| r.clone())
    }

    pub fn find_by_name(&self, name: String) -> Vec<Tag> {
        if let Some(&idx) = self.name_index.get(&name) {
            if !self.tombstones[idx] {
                return vec![self.records[idx].clone()];
            }
        }
        Vec::new()
    }

    pub fn list(&self) -> Vec<Tag> {
        self.records.iter().enumerate()
            .filter(|(i, _)| !self.tombstones[*i])
            .map(|(_, r)| r.clone())
            .collect()
    }

    pub fn update(&mut self, id: uuid::Uuid, name: String) -> Result<Tag, String> {
        let idx = self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(i, _)| i)
            .ok_or_else(|| "Record not found".to_string())?;

        self.name_index.remove(&self.records[idx].name);
        if self.name_index.contains_key(&name) && self.records[idx].name != name {
            return Err("Unique constraint violation: name already exists".to_string());
        }
        self.records[idx] = Tag {
            id: self.records[idx].id.clone(),
            name,
        };

        self.name_index.insert(self.records[idx].name.clone(), idx);
        Ok(self.records[idx].clone())
    }

    pub fn delete(&mut self, id: uuid::Uuid) -> Result<(), String> {
        let idx = self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(i, _)| i)
            .ok_or_else(|| "Record not found".to_string())?;

        self.tombstones[idx] = true;

        self.name_index.remove(&self.records[idx].name);
        Ok(())
    }

}

