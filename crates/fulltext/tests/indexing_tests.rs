use forgedb_fulltext::*;
use uuid::Uuid;

#[test]
fn test_add_single_document() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "single document");
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 1);
    assert_eq!(stats.total_terms, 2);
}

#[test]
fn test_add_multiple_documents() {
    let mut index = FullTextIndex::new();
    
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let doc3 = Uuid::new_v4();
    
    index.add_document(doc1, "first document");
    index.add_document(doc2, "second document");
    index.add_document(doc3, "third document");
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 3);
}

#[test]
fn test_add_empty_document() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "");
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 1);
    assert_eq!(stats.total_terms, 0);
}

#[test]
fn test_add_document_with_duplicates() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "test test test");
    
    let results = index.search("test");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc_id);
}

#[test]
fn test_add_document_with_special_chars() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "hello! world? test-case_name");
    
    let results = index.search("hello");
    assert_eq!(results.len(), 1);
    
    let results = index.search("test-case_name");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_remove_document_basic() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "test document");
    assert_eq!(index.stats().total_docs, 1);
    
    index.remove_document(doc_id, "test document");
    assert_eq!(index.stats().total_docs, 0);
}

#[test]
fn test_remove_nonexistent_document() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "test document");
    
    // Remove a different document
    index.remove_document(doc2, "test document");
    
    // Original document should still be there
    let results = index.search("test");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc1);
}

#[test]
fn test_remove_one_of_multiple_documents() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let doc3 = Uuid::new_v4();

    index.add_document(doc1, "rust programming");
    index.add_document(doc2, "rust language");
    index.add_document(doc3, "python programming");
    
    index.remove_document(doc2, "rust language");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc1);
}

#[test]
fn test_add_document_updates_trigrams() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "hello");
    
    let stats = index.stats();
    assert!(stats.total_trigrams > 0);
}

#[test]
fn test_index_with_long_text() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    let long_text = "This is a very long document with many words. \
                     It contains multiple sentences and various terms. \
                     The index should handle this efficiently and correctly.";
    
    index.add_document(doc_id, long_text);
    
    let results = index.search("efficiently");
    assert_eq!(results.len(), 1);
    
    let results = index.search("sentences");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_batch_indexing() {
    let mut index = FullTextIndex::new();
    let docs = vec![
        (Uuid::new_v4(), "document one"),
        (Uuid::new_v4(), "document two"),
        (Uuid::new_v4(), "document three"),
        (Uuid::new_v4(), "document four"),
        (Uuid::new_v4(), "document five"),
    ];
    
    for (id, text) in &docs {
        index.add_document(*id, text);
    }
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 5);
    
    let results = index.search("document");
    assert_eq!(results.len(), 5);
}

#[test]
fn test_reindex_same_document() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "original content");
    
    // "Update" by removing and re-adding
    index.remove_document(doc_id, "original content");
    index.add_document(doc_id, "updated content");
    
    let results = index.search("updated");
    assert_eq!(results.len(), 1);
    
    let results = index.search("original");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_index_case_insensitivity() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "RuSt Programming");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 1);
    
    let results = index.search("PROGRAMMING");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_index_multiple_positions() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "the quick brown fox jumps over the lazy dog");
    
    let results = index.search("the");
    assert_eq!(results.len(), 1);
    // "the" appears at positions 0 and 6
    assert!(results[0].positions.len() >= 2);
}

#[test]
fn test_index_stats_accuracy() {
    let mut index = FullTextIndex::new();
    
    index.add_document(Uuid::new_v4(), "one two three");
    index.add_document(Uuid::new_v4(), "four five six");
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 2);
    // Should have 6 unique terms
    assert_eq!(stats.total_terms, 6);
}

#[test]
fn test_remove_all_documents() {
    let mut index = FullTextIndex::new();
    let docs = vec![
        (Uuid::new_v4(), "doc one"),
        (Uuid::new_v4(), "doc two"),
        (Uuid::new_v4(), "doc three"),
    ];
    
    for (id, text) in &docs {
        index.add_document(*id, text);
    }
    
    for (id, text) in &docs {
        index.remove_document(*id, text);
    }
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 0);
    
    let results = index.search("doc");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_index_with_numbers() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "version 1.0 released");
    
    let results = index.search("version");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_index_unicode_text() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    // Test with basic unicode characters
    index.add_document(doc_id, "hello café résumé");
    
    let stats = index.stats();
    assert_eq!(stats.total_docs, 1);
    
    // Should be able to search for the terms
    let results = index.search("hello");
    assert_eq!(results.len(), 1);
}
