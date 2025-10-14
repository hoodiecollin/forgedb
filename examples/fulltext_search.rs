// Sprint 18: Full-Text Search Example
//
// Demonstrates:
// - @fulltext directive for text fields
// - search_<field>() methods for relevance-ranked search
// - search_<field>_phrase() methods for exact phrase matching
// - TF-IDF scoring for search results

use sinkdb::{Parser, CodeGenerator};

fn main() -> Result<(), String> {
    let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
  content: string @fulltext
  author: string
  published: timestamp
}
"#;

    // Parse schema
    let mut parser = Parser::new(schema)?;
    let parsed_schema = parser.parse()?;

    // Generate code
    let generator = CodeGenerator::new();
    let generated_code = generator.generate(&parsed_schema);

    println!("\nFull-text search features:");
    println!("- Parse @fulltext directive on string fields");
    println!("- Generate search_<field>() methods with TF-IDF ranking");
    println!("- Generate search_<field>_phrase() methods for exact matches");
    println!("- Automatically maintain full-text indexes on insert/update/delete");

    // Verify full-text index fields
    assert!(generated_code.contains("title_fulltext"),
        "Missing title_fulltext index field");
    assert!(generated_code.contains("content_fulltext"),
        "Missing content_fulltext index field");

    // Verify search methods
    assert!(generated_code.contains("pub fn search_title"),
        "Missing search_title method");
    assert!(generated_code.contains("pub fn search_content"),
        "Missing search_content method");
    assert!(generated_code.contains("pub fn search_title_phrase"),
        "Missing search_title_phrase method");
    assert!(generated_code.contains("pub fn search_content_phrase"),
        "Missing search_content_phrase method");

    // Verify full-text import
    assert!(generated_code.contains("sinkdb_fulltext"),
        "Missing sinkdb_fulltext import");

    println!("\n✅ All full-text search features generated successfully!");
    println!("\nExample usage:");
    println!("  let mut storage = ArticleStorage::new();");
    println!("  storage.insert(\"Rust Tutorial\".to_string(), \"Learn Rust programming...\".to_string(), ...);");
    println!("  let results = storage.search_title(\"rust\");");
    println!("  let exact = storage.search_content_phrase(\"Rust programming\");");

    Ok(())
}
