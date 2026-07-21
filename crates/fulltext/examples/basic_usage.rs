//! Basic usage example for forgedb-fulltext
//!
//! This example demonstrates creating a full-text search index,
//! adding documents, and searching with TF-IDF scoring.

use forgedb_fulltext::*;
use uuid::Uuid;

fn main() {
    println!("=== ForgeDB Full-Text Search - Basic Usage ===\n");

    // Create a new full-text index
    let mut index = FullTextIndex::new();
    println!("✓ Created full-text index\n");

    // Add some documents
    println!("--- Adding Documents ---");
    
    let doc1_id = Uuid::new_v4();
    let doc1_text = "Rust is a systems programming language focused on safety and performance";
    index.add_document(doc1_id, doc1_text);
    println!("Added document 1: {}", doc1_text);

    let doc2_id = Uuid::new_v4();
    let doc2_text = "ForgeDB is a high-performance database written in Rust";
    index.add_document(doc2_id, doc2_text);
    println!("Added document 2: {}", doc2_text);

    let doc3_id = Uuid::new_v4();
    let doc3_text = "Database systems require careful attention to performance and safety";
    index.add_document(doc3_id, doc3_text);
    println!("Added document 3: {}", doc3_text);

    let doc4_id = Uuid::new_v4();
    let doc4_text = "Rust provides memory safety without garbage collection";
    index.add_document(doc4_id, doc4_text);
    println!("Added document 4: {}\n", doc4_text);

    // Get index statistics
    println!("--- Index Statistics ---");
    let stats = index.stats();
    println!("Total documents: {}", stats.total_docs);
    println!("Total unique terms: {}", stats.total_terms);
    println!("Total trigrams: {}\n", stats.total_trigrams);

    // Example 1: Search for "Rust"
    println!("--- Search: 'Rust' ---");
    let results1 = index.search("Rust");
    println!("Found {} results:", results1.len());
    for (i, result) in results1.iter().enumerate() {
        println!("  {}. Document {} (score: {:.4})", i + 1, result.doc_id, result.score);
        println!("     Positions: {:?}", result.positions);
    }
    println!();

    // Example 2: Search for "performance"
    println!("--- Search: 'performance' ---");
    let results2 = index.search("performance");
    println!("Found {} results:", results2.len());
    for (i, result) in results2.iter().enumerate() {
        println!("  {}. Document {} (score: {:.4})", i + 1, result.doc_id, result.score);
    }
    println!();

    // Example 3: Search for multiple terms (OR semantics)
    println!("--- Search: 'database systems' ---");
    let results3 = index.search("database systems");
    println!("Found {} results (documents containing 'database' OR 'systems'):", results3.len());
    for (i, result) in results3.iter().enumerate() {
        println!("  {}. Document {} (score: {:.4})", i + 1, result.doc_id, result.score);
    }
    println!();

    // Example 4: Phrase search (exact phrase matching)
    println!("--- Phrase Search: 'memory safety' ---");
    let results4 = index.search_phrase("memory safety");
    println!("Found {} exact phrase matches:", results4.len());
    for (i, result) in results4.iter().enumerate() {
        println!("  {}. Document {} (score: {:.4})", i + 1, result.doc_id, result.score);
        println!("     Phrase at positions: {:?}", result.positions);
    }
    println!();

    // Example 5: Search with no results
    println!("--- Search: 'javascript' (no results expected) ---");
    let results5 = index.search("javascript");
    println!("Found {} results", results5.len());
    if results5.is_empty() {
        println!("No documents match the query");
    }
    println!();

    // Example 6: Remove a document and search again
    println!("--- Removing Document ---");
    index.remove_document(doc2_id, doc2_text);
    println!("Removed document 2");
    
    let stats_after = index.stats();
    println!("Updated statistics:");
    println!("  Total documents: {}", stats_after.total_docs);
    println!("  Total unique terms: {}\n", stats_after.total_terms);

    // Search again after removal
    println!("--- Search After Removal: 'Rust' ---");
    let results6 = index.search("Rust");
    println!("Found {} results (down from {}):", results6.len(), results1.len());
    for (i, result) in results6.iter().enumerate() {
        println!("  {}. Document {} (score: {:.4})", i + 1, result.doc_id, result.score);
    }

    println!("\n✓ Example completed successfully!");
}
