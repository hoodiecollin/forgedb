// Generated code - do not edit manually

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: uuid::Uuid,
    pub title: String,
    pub author_id: uuid::Uuid,
}

pub struct PostStorage {
    records: Vec<Post>,
    next_id: u64,
    tombstones: Vec<bool>,
    author_id_index: std::collections::HashMap<uuid::Uuid, Vec<usize>>,
}

impl PostStorage {
    pub fn new() -> Self {
        PostStorage {
            records: Vec::new(),
            next_id: 1,
            tombstones: Vec::new(),
            author_id_index: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, title: String, author_id: uuid::Uuid) -> Result<Post, String> {
        let id = Uuid::new_v4();
        let record = Post {
            id,
            title,
            author_id,
        };

        let row_index = self.records.len();
        self.author_id_index.entry(record.author_id.clone()).or_insert_with(Vec::new).push(row_index);
        self.records.push(record.clone());
        self.tombstones.push(false);
        Ok(record)
    }

    pub fn get(&self, id: uuid::Uuid) -> Option<Post> {
        self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(_, r)| r.clone())
    }

    pub fn find_by_author_id(&self, author_id: uuid::Uuid) -> Vec<Post> {
        if let Some(indices) = self.author_id_index.get(&author_id) {
            return indices.iter()
                .filter(|&&idx| !self.tombstones[idx])
                .map(|&idx| self.records[idx].clone())
                .collect();
        }
        Vec::new()
    }

    pub fn list(&self) -> Vec<Post> {
        self.records.iter().enumerate()
            .filter(|(i, _)| !self.tombstones[*i])
            .map(|(_, r)| r.clone())
            .collect()
    }

    pub fn update(&mut self, id: uuid::Uuid, title: String, author_id: uuid::Uuid) -> Result<Post, String> {
        let idx = self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(i, _)| i)
            .ok_or_else(|| "Record not found".to_string())?;

        if let Some(indices) = self.author_id_index.get_mut(&self.records[idx].author_id) {
            indices.retain(|&i| i != idx);
        }
        self.records[idx] = Post {
            id: self.records[idx].id.clone(),
            title,
            author_id,
        };

        self.author_id_index.entry(self.records[idx].author_id.clone()).or_insert_with(Vec::new).push(idx);
        Ok(self.records[idx].clone())
    }

    pub fn delete(&mut self, id: uuid::Uuid) -> Result<(), String> {
        let idx = self.records.iter().enumerate()
            .find(|(i, r)| !self.tombstones[*i] && r.id == id)
            .map(|(i, _)| i)
            .ok_or_else(|| "Record not found".to_string())?;

        self.tombstones[idx] = true;

        if let Some(indices) = self.author_id_index.get_mut(&self.records[idx].author_id) {
            indices.retain(|&i| i != idx);
        }
        Ok(())
    }

}

