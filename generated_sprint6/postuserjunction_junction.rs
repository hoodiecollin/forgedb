// Generated code - do not edit manually

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct PostUserJunction {
    pub post_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
}

pub struct PostUserJunctionStorage {
    records: Vec<PostUserJunction>,
    post_to_user_index: std::collections::HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
    user_to_post_index: std::collections::HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
}

impl PostUserJunctionStorage {
    pub fn new() -> Self {
        PostUserJunctionStorage {
            records: Vec::new(),
            post_to_user_index: std::collections::HashMap::new(),
            user_to_post_index: std::collections::HashMap::new(),
        }
    }

    pub fn add_relation(&mut self, post_id: uuid::Uuid, user_id: uuid::Uuid) {
        // Check if relation already exists
        if self.has_relation(post_id, user_id) {
            return;
        }

        let record = PostUserJunction {
            post_id,
            user_id,
        };
        self.records.push(record);
        self.post_to_user_index.entry(post_id).or_insert_with(Vec::new).push(user_id);
        self.user_to_post_index.entry(user_id).or_insert_with(Vec::new).push(post_id);
    }

    pub fn remove_relation(&mut self, post_id: uuid::Uuid, user_id: uuid::Uuid) {
        self.records.retain(|r| !(r.post_id == post_id && r.user_id == user_id));
        if let Some(ids) = self.post_to_user_index.get_mut(&post_id) {
            ids.retain(|&id| id != user_id);
        }
        if let Some(ids) = self.user_to_post_index.get_mut(&user_id) {
            ids.retain(|&id| id != post_id);
        }
    }

    pub fn get_post_liked_by(&self, post_id: uuid::Uuid) -> Vec<uuid::Uuid> {
        self.post_to_user_index.get(&post_id)
            .map(|ids| ids.clone())
            .unwrap_or_else(Vec::new)
    }

    pub fn get_user_liked_posts(&self, user_id: uuid::Uuid) -> Vec<uuid::Uuid> {
        self.user_to_post_index.get(&user_id)
            .map(|ids| ids.clone())
            .unwrap_or_else(Vec::new)
    }

    pub fn has_relation(&self, post_id: uuid::Uuid, user_id: uuid::Uuid) -> bool {
        self.records.iter().any(|r| r.post_id == post_id && r.user_id == user_id)
    }
}

