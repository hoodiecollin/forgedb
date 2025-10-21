//! Intermediate example for forgedb-fulltext
//!
//! This example demonstrates building a simple search application
//! with document management and advanced search features.

use forgedb_fulltext::*;
use std::collections::HashMap;
use uuid::Uuid;

// Document metadata
#[derive(Debug, Clone)]
struct Document {
    id: Uuid,
    title: String,
    content: String,
    author: String,
}

// Simple document store with full-text search
struct SearchEngine {
    index: FullTextIndex,
    documents: HashMap<Uuid, Document>,
}

impl SearchEngine {
    fn new() -> Self {
        Self {
            index: FullTextIndex::new(),
            documents: HashMap::new(),
        }
    }

    fn add_document(&mut self, title: String, content: String, author: String) -> Uuid {
        let id = Uuid::new_v4();
        
        // Index the full text (title + content)
        let searchable_text = format!("{} {}", title, content);
        self.index.add_document(id, &searchable_text);

        // Store document metadata
        self.documents.insert(
            id,
            Document {
                id,
                title,
                content,
                author,
            },
        );

        id
    }

    fn remove_document(&mut self, id: Uuid) -> Option<Document> {
        if let Some(doc) = self.documents.remove(&id) {
            let searchable_text = format!("{} {}", doc.title, doc.content);
            self.index.remove_document(id, &searchable_text);
            Some(doc)
        } else {
            None
        }
    }

    fn search(&self, query: &str) -> Vec<(&Document, f64)> {
        let results = self.index.search(query);
        
        results
            .iter()
            .filter_map(|result| {
                self.documents
                    .get(&result.doc_id)
                    .map(|doc| (doc, result.score))
            })
            .collect()
    }

    fn search_phrase(&self, phrase: &str) -> Vec<(&Document, f64)> {
        let results = self.index.search_phrase(phrase);
        
        results
            .iter()
            .filter_map(|result| {
                self.documents
                    .get(&result.doc_id)
                    .map(|doc| (doc, result.score))
            })
            .collect()
    }

    fn stats(&self) -> (usize, IndexStats) {
        (self.documents.len(), self.index.stats())
    }
}

fn main() {
    println!("=== ForgeDB Full-Text Search - Search Application ===\n");

    let mut engine = SearchEngine::new();
    println!("✓ Search engine initialized\n");

    // Add blog posts
    println!("--- Adding Blog Posts ---");
    
    engine.add_document(
        "Introduction to Rust".to_string(),
        "Rust is a modern systems programming language that focuses on safety, \
         speed, and concurrency. It achieves memory safety without garbage collection.".to_string(),
        "Alice".to_string(),
    );
    println!("✓ Added: Introduction to Rust");

    engine.add_document(
        "Building Databases in Rust".to_string(),
        "ForgeDB is a high-performance database system built entirely in Rust. \
         It leverages Rust's safety guarantees to prevent common database bugs.".to_string(),
        "Bob".to_string(),
    );
    println!("✓ Added: Building Databases in Rust");

    engine.add_document(
        "Memory Safety Without GC".to_string(),
        "Rust's ownership system ensures memory safety at compile time, eliminating \
         the need for a garbage collector and enabling predictable performance.".to_string(),
        "Alice".to_string(),
    );
    println!("✓ Added: Memory Safety Without GC");

    engine.add_document(
        "Concurrent Programming Patterns".to_string(),
        "Rust makes concurrent programming safe and efficient through its type system. \
         The borrow checker prevents data races at compile time.".to_string(),
        "Charlie".to_string(),
    );
    println!("✓ Added: Concurrent Programming Patterns");

    engine.add_document(
        "Performance Optimization Tips".to_string(),
        "Optimizing Rust code for performance involves understanding zero-cost abstractions, \
         avoiding unnecessary allocations, and leveraging SIMD when appropriate.".to_string(),
        "Bob".to_string(),
    );
    println!("✓ Added: Performance Optimization Tips\n");

    // Get statistics
    let (doc_count, index_stats) = engine.stats();
    println!("--- Statistics ---");
    println!("Documents indexed: {}", doc_count);
    println!("Unique terms: {}", index_stats.total_terms);
    println!("Trigrams: {}\n", index_stats.total_trigrams);

    // Search examples
    println!("--- Search 1: 'Rust safety' ---");
    let results1 = engine.search("Rust safety");
    println!("Found {} results:", results1.len());
    for (i, (doc, score)) in results1.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, doc.title, score);
        println!("     by {}", doc.author);
    }
    println!();

    println!("--- Search 2: 'performance' ---");
    let results2 = engine.search("performance");
    println!("Found {} results:", results2.len());
    for (i, (doc, score)) in results2.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, doc.title, score);
        println!("     by {}", doc.author);
    }
    println!();

    println!("--- Search 3: 'database' ---");
    let results3 = engine.search("database");
    println!("Found {} results:", results3.len());
    for (i, (doc, score)) in results3.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, doc.title, score);
        println!("     by {}", doc.author);
        println!("     Content preview: {}...", &doc.content[..60]);
    }
    println!();

    // Phrase search
    println!("--- Phrase Search: 'memory safety' ---");
    let results4 = engine.search_phrase("memory safety");
    println!("Found {} exact phrase matches:", results4.len());
    for (i, (doc, score)) in results4.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, doc.title, score);
        println!("     by {}", doc.author);
    }
    println!();

    // Search and display full content
    println!("--- Detailed Search: 'concurrent' ---");
    let results5 = engine.search("concurrent");
    println!("Found {} results:\n", results5.len());
    for (doc, score) in results5.iter() {
        println!("Title: {}", doc.title);
        println!("Author: {}", doc.author);
        println!("Score: {:.4}", score);
        println!("Content: {}", doc.content);
        println!("{}", "-".repeat(60));
    }

    // Remove a document and search again
    println!("\n--- Removing a Document ---");
    let docs_to_remove: Vec<Uuid> = engine
        .documents
        .values()
        .filter(|d| d.title.contains("Performance"))
        .map(|d| d.id)
        .collect();
    
    for id in docs_to_remove {
        if let Some(removed) = engine.remove_document(id) {
            println!("✓ Removed: {}", removed.title);
        }
    }

    let (final_count, final_stats) = engine.stats();
    println!("\nFinal statistics:");
    println!("  Documents: {}", final_count);
    println!("  Terms: {}", final_stats.total_terms);

    println!("\n✓ Example completed successfully!");
}
