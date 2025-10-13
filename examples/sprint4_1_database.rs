// Sprint 4.1: Database with FK Validation and Relation Traversal
// Demonstrates the new Database struct with:
// - Foreign key validation
// - Relation traversal methods (user.posts())
// - Reverse lookup methods (post.author())

// Note: This example shows the API, but we can't actually run the generated
// code in this example without more setup. See the generated code in
// generated/sprint4_database.rs for the full implementation.

fn main() {
    println!("=== Sprint 4.1: Database API Example ===\n");

    println!("Schema:");
    println!(r#"
User {{
  id: +uuid
  email: ^&string
  posts: [Post]
}}

Post {{
  id: +uuid
  title: string
  author: *User
}}
"#);

    println!("\n=== Generated Database Struct ===");
    println!(r#"
pub struct Database {{
    pub user: UserStorage,
    pub post: PostStorage,
}}

impl Database {{
    pub fn new() -> Self {{ ... }}

    // FK Validation on Insert
    pub fn insert_post(&mut self,
        title: String,
        content: String,
        author_id: uuid::Uuid
    ) -> Result<Post, String> {{
        // Validates that author_id references an existing User
        if self.user.get(author_id).is_none() {{
            return Err("Foreign key validation failed: User does not exist".to_string());
        }}
        self.post.insert(title, content, author_id)
    }}

    // Relation Traversal: Get all posts for a user
    pub fn user_posts(&self, user_id: uuid::Uuid) -> Vec<Post> {{
        self.post.find_by_user_id(user_id)
    }}

    // Reverse Lookup: Get the author of a post
    pub fn post_author(&self, post_id: uuid::Uuid) -> Option<User> {{
        if let Some(child) = self.post.get(post_id) {{
            return self.user.get(child.author_id);
        }}
        None
    }}
}}
"#);

    println!("\n=== Example Usage ===");
    println!(r#"
let mut db = Database::new();

// Insert a user
let user = db.user.insert(
    "alice@example.com".to_string(),
    "Alice".to_string()
).unwrap();

// Insert a post with FK validation
let post = db.insert_post(
    "My First Post".to_string(),
    "Hello World!".to_string(),
    user.id  // References the user
).unwrap();

// FK validation prevents invalid references
let result = db.insert_post(
    "Invalid Post".to_string(),
    "Content".to_string(),
    Uuid::new_v4()  // Non-existent user ID
);
assert!(result.is_err());  // ✗ FK validation fails

// Traverse relations: Get all posts by user
let posts = db.user_posts(user.id);
assert_eq!(posts.len(), 1);

// Reverse lookup: Get post's author
let author = db.post_author(post.id).unwrap();
assert_eq!(author.id, user.id);
"#);

    println!("\n=== Key Features ===");
    println!("✅ Database struct holds all model storages");
    println!("✅ FK validation prevents orphaned records");
    println!("✅ Relation traversal methods (user.posts())");
    println!("✅ Reverse lookup methods (post.author())");
    println!("✅ Type-safe API generated from schema");

    println!("\n=== Sprint 4.1 Complete! ===");
    println!("All deferred Sprint 4 features have been implemented:");
    println!("  ✓ Runtime FK validation");
    println!("  ✓ Relation traversal methods");
    println!("  ✓ Reverse lookup methods");
    println!("\nSee generated/sprint4_database.rs for the full generated code.");
}
