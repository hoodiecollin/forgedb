/// Sprint 6: Multiple Models & Many-to-Many Relations Example
///
/// This example demonstrates:
/// - Multiple models in a single schema
/// - Many-to-many relationships detected from bidirectional OneToMany fields
/// - Junction table generation and operations
/// - Multi-file code generation
///
/// Schema:
/// ```
/// User {
///   id: +uuid
///   posts: [Post]
///   liked_posts: [Post]
/// }
///
/// Post {
///   id: +uuid
///   author: *User
///   tags: [Tag]
///   liked_by: [User]
/// }
///
/// Tag {
///   id: +uuid
///   posts: [Post]
/// }
/// ```
///
/// Many-to-Many Relations Detected:
/// - User.liked_posts <-> Post.liked_by (User likes many Posts, Post liked by many Users)
/// - Post.tags <-> Tag.posts (Post has many Tags, Tag applied to many Posts)

use sinkdb::parser::Parser;
use sinkdb::codegen::CodeGenerator;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Sprint 6: Many-to-Many Relations Example ===\n");

    // Define schema with multiple models and M:N relations
    let schema_str = r#"
User {
  id: +uuid
  email: &string
  posts: [Post]
  liked_posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
  tags: [Tag]
  liked_by: [User]
}

Tag {
  id: +uuid
  name: &string
  posts: [Post]
}
"#;

    println!("Schema:\n{}\n", schema_str);

    // Parse schema
    let mut parser = Parser::new(schema_str)?;
    let schema = parser.parse()?;

    println!("Parsed {} models:", schema.models.len());
    for model in &schema.models {
        println!("  - {}", model.name);
    }
    println!();

    // Show 1:N relations first
    let one_to_many_rels = schema.detect_relations();
    println!("Detected {} one-to-many (FK-based) relations:", one_to_many_rels.len());
    for rel in &one_to_many_rels {
        println!("  - {}.{} (OneToMany) <-> {}.{} (FK)",
            rel.parent_model, rel.parent_field, rel.child_model, rel.child_field);
    }
    println!();

    // Detect many-to-many relations
    let m2m_relations = schema.detect_many_to_many_relations();
    println!("Detected {} many-to-many relations:", m2m_relations.len());
    for m2m in &m2m_relations {
        println!("  - {}.{} <-> {}.{}",
            m2m.model1, m2m.field1, m2m.model2, m2m.field2);
    }
    println!();

    // Generate multi-file output
    let generator = CodeGenerator::new();
    let files = generator.generate_files(&schema);

    println!("Generated {} files:", files.len());
    for file in &files {
        println!("  - {}", file.path);
    }
    println!();

    // Create output directory
    let output_dir = "generated_sprint6";
    fs::create_dir_all(output_dir)?;
    println!("Writing files to {}/", output_dir);

    // Write all generated files
    for file in &files {
        let file_path = format!("{}/{}", output_dir, file.path);
        fs::write(&file_path, &file.content)?;
        println!("  Wrote: {}", file_path);
    }
    println!();

    // Show sample of generated junction table
    println!("=== Sample: PostTagJunction ===");
    if let Some(junction_file) = files.iter().find(|f| f.path.contains("junction")) {
        let lines: Vec<&str> = junction_file.content.lines().take(50).collect();
        for line in lines {
            println!("{}", line);
        }
        if junction_file.content.lines().count() > 50 {
            println!("... (truncated)");
        }
    }
    println!();

    // Show sample of database.rs
    println!("=== Sample: database.rs ===");
    if let Some(db_file) = files.iter().find(|f| f.path == "database.rs") {
        let lines: Vec<&str> = db_file.content.lines().take(40).collect();
        for line in lines {
            println!("{}", line);
        }
        if db_file.content.lines().count() > 40 {
            println!("... (truncated)");
        }
    }
    println!();

    println!("=== Example Usage Scenario ===");
    println!(r#"
// Create database
let mut db = Database::new();

// Create users
let alice = db.user.insert("alice@example.com".to_string())?;
let bob = db.user.insert("bob@example.com".to_string())?;

// Create posts
let post1 = db.post.insert("My First Post".to_string(), alice.id)?;
let post2 = db.post.insert("Rust Tips".to_string(), bob.id)?;

// Create tags
let tag_rust = db.tag.insert("rust".to_string())?;
let tag_tutorial = db.tag.insert("tutorial".to_string())?;

// Add many-to-many relations

// Bob likes Alice's post (User <-> Post liked_by)
db.post_liked_posts.add_relation(bob.id, post1.id);

// Alice likes Bob's post
db.post_liked_posts.add_relation(alice.id, post2.id);

// Tag posts (Post <-> Tag)
db.post_tags.add_relation(post2.id, tag_rust.id);
db.post_tags.add_relation(post2.id, tag_tutorial.id);

// Query many-to-many relations

// Get posts liked by Bob
let bobs_liked_posts = db.post_liked_posts.get_user_liked_posts(bob.id);
println!("Bob likes {{}} posts", bobs_liked_posts.len());

// Get users who liked post1
let likers = db.post_liked_posts.get_post_liked_by(post1.id);
println!("Post 1 is liked by {{}} users", likers.len());

// Get tags for post2
let post2_tags = db.post_tags.get_post_tags(post2.id);
println!("Post 2 has {{}} tags", post2_tags.len());

// Get posts with rust tag
let rust_posts = db.post_tags.get_tag_posts(tag_rust.id);
println!("Rust tag applied to {{}} posts", rust_posts.len());

// Check if relation exists
let has_relation = db.post_tags.has_relation(post2.id, tag_rust.id);
println!("Post 2 tagged with rust: {{}}", has_relation);

// Remove relation
db.post_liked_posts.remove_relation(bob.id, post1.id);
println!("Removed Bob's like from post 1");
"#);

    println!("\n=== Success! ===");
    println!("Sprint 6 demonstrates:");
    println!("  - Multiple models in schema");
    println!("  - Many-to-many relation detection from bidirectional OneToMany");
    println!("  - Junction table generation with add/remove/query operations");
    println!("  - Multi-file code organization");
    println!("  - Database struct managing all models and junctions");

    Ok(())
}
