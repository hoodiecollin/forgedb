// Generated code - do not edit manually

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct PostTagJunction {
    pub post_id: uuid::Uuid,
    pub tag_id: uuid::Uuid,
}

pub struct PostTagJunctionStorage {
    records: Vec<PostTagJunction>,
    post_to_tag_index: std::collections::HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
    tag_to_post_index: std::collections::HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
}

impl PostTagJunctionStorage {
    pub fn new() -> Self {
        PostTagJunctionStorage {
            records: Vec::new(),
            post_to_tag_index: std::collections::HashMap::new(),
            tag_to_post_index: std::collections::HashMap::new(),
        }
    }

    pub fn add_relation(&mut self, post_id: uuid::Uuid, tag_id: uuid::Uuid) {
        // Check if relation already exists
        if self.has_relation(post_id, tag_id) {
            return;
        }

        let record = PostTagJunction {
            post_id,
            tag_id,
        };
        self.records.push(record);
        self.post_to_tag_index.entry(post_id).or_insert_with(Vec::new).push(tag_id);
        self.tag_to_post_index.entry(tag_id).or_insert_with(Vec::new).push(post_id);
    }

    pub fn remove_relation(&mut self, post_id: uuid::Uuid, tag_id: uuid::Uuid) {
        self.records.retain(|r| !(r.post_id == post_id && r.tag_id == tag_id));
        if let Some(ids) = self.post_to_tag_index.get_mut(&post_id) {
            ids.retain(|&id| id != tag_id);
        }
        if let Some(ids) = self.tag_to_post_index.get_mut(&tag_id) {
            ids.retain(|&id| id != post_id);
        }
    }

    pub fn get_post_tags(&self, post_id: uuid::Uuid) -> Vec<uuid::Uuid> {
        self.post_to_tag_index.get(&post_id)
            .map(|ids| ids.clone())
            .unwrap_or_else(Vec::new)
    }

    pub fn get_tag_posts(&self, tag_id: uuid::Uuid) -> Vec<uuid::Uuid> {
        self.tag_to_post_index.get(&tag_id)
            .map(|ids| ids.clone())
            .unwrap_or_else(Vec::new)
    }

    pub fn has_relation(&self, post_id: uuid::Uuid, tag_id: uuid::Uuid) -> bool {
        self.records.iter().any(|r| r.post_id == post_id && r.tag_id == tag_id)
    }
}

