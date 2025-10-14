// Generated code - do not edit manually

use super::*;

pub struct Database {
    pub user: UserStorage,
    pub post: PostStorage,
    pub tag: TagStorage,
    pub post_liked_by: PostUserJunctionStorage,
    pub post_liked_by: PostUserJunctionStorage,
    pub post_tags: PostTagJunctionStorage,
}

impl Database {
    pub fn new() -> Self {
        Database {
            user: UserStorage::new(),
            post: PostStorage::new(),
            tag: TagStorage::new(),
            post_liked_by: PostUserJunctionStorage::new(),
            post_liked_by: PostUserJunctionStorage::new(),
            post_tags: PostTagJunctionStorage::new(),
        }
    }

    pub fn insert_post(&mut self, title: String, author_id: uuid::Uuid) -> Result<Post, String> {
        if self.user.get(author_id).is_none() {
            return Err("Foreign key validation failed: User does not exist".to_string());
        }
        self.post.insert(title, author_id)
    }

    pub fn user_posts(&self, user_id: uuid::Uuid) -> Vec<Post> {
        self.post.find_by_user_id(user_id)
    }

    pub fn post_author(&self, post_id: uuid::Uuid) -> Option<User> {
        if let Some(child) = self.post.get(post_id) {
            return self.user.get(child.author_id);
        }
        None
    }

    pub fn user_liked_posts(&self, user_id: uuid::Uuid) -> Vec<Post> {
        self.post.find_by_user_id(user_id)
    }

    pub fn post_author(&self, post_id: uuid::Uuid) -> Option<User> {
        if let Some(child) = self.post.get(post_id) {
            return self.user.get(child.author_id);
        }
        None
    }

}

