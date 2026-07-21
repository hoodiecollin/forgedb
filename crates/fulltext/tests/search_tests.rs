use forgedb_fulltext::*;
use uuid::Uuid;

#[test]
fn test_search_single_term() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "hello world");
    
    let results = index.search("hello");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc_id);
}

#[test]
fn test_search_no_matches() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "hello world");
    
    let results = index.search("nonexistent");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_search_multiple_terms() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "rust programming language");
    
    let results = index.search("rust programming");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_returns_all_matching_docs() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let doc3 = Uuid::new_v4();

    index.add_document(doc1, "rust is great");
    index.add_document(doc2, "rust is fast");
    index.add_document(doc3, "python is slow");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_search_empty_query() {
    let mut index = FullTextIndex::new();
    index.add_document(Uuid::new_v4(), "test document");
    
    let results = index.search("");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_search_empty_index() {
    let index = FullTextIndex::new();
    
    let results = index.search("anything");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_search_case_insensitive() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "Rust Programming");
    
    let results1 = index.search("rust");
    let results2 = index.search("RUST");
    let results3 = index.search("RuSt");
    
    assert_eq!(results1.len(), 1);
    assert_eq!(results2.len(), 1);
    assert_eq!(results3.len(), 1);
}

#[test]
fn test_search_results_sorted_by_score() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    // doc1 has "rust" three times
    index.add_document(doc1, "rust rust rust");
    // doc2 has "rust" once
    index.add_document(doc2, "rust");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 2);
    
    // Higher score should come first
    let doc1_result = results.iter().find(|r| r.doc_id == doc1).unwrap();
    let doc2_result = results.iter().find(|r| r.doc_id == doc2).unwrap();
    assert!(doc1_result.score > doc2_result.score);
}

#[test]
fn test_search_with_punctuation() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "Hello, world! How are you?");
    
    let results = index.search("hello");
    assert_eq!(results.len(), 1);
    
    let results = index.search("world");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_partial_match() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "programming in rust");
    
    // Search for all terms
    let results = index.search("programming rust");
    assert_eq!(results.len(), 1);
    
    // Search for one term
    let results = index.search("programming");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_with_positions() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "one two three");
    
    let results = index.search("two");
    assert_eq!(results.len(), 1);
    assert!(results[0].positions.contains(&1));
}

#[test]
fn test_phrase_search_exact_match() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "quick brown fox");
    index.add_document(doc2, "brown quick fox");
    
    let results = index.search_phrase("quick brown");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc1);
}

#[test]
fn test_phrase_search_no_match() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "hello world");
    
    let results = index.search_phrase("world hello");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_phrase_search_single_word() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "hello world");
    
    let results = index.search_phrase("hello");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_phrase_search_empty() {
    let mut index = FullTextIndex::new();
    index.add_document(Uuid::new_v4(), "test");
    
    let results = index.search_phrase("");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_phrase_search_multiword() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "the quick brown fox jumps over the lazy dog");
    
    let results = index.search_phrase("quick brown fox");
    assert_eq!(results.len(), 1);
    
    let results = index.search_phrase("fox jumps over");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_phrase_search_with_repeated_words() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "test test test");
    
    let results = index.search_phrase("test test");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_common_words() {
    let mut index = FullTextIndex::new();
    
    index.add_document(Uuid::new_v4(), "the cat sat on the mat");
    index.add_document(Uuid::new_v4(), "the dog ran in the park");
    index.add_document(Uuid::new_v4(), "the bird flew over the tree");
    
    let results = index.search("the");
    assert_eq!(results.len(), 3);
}

#[test]
fn test_search_scoring_frequency() {
    let mut index = FullTextIndex::new();
    let high_freq = Uuid::new_v4();
    let low_freq = Uuid::new_v4();

    index.add_document(high_freq, "rust rust rust rust rust");
    index.add_document(low_freq, "rust programming");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 2);
    
    // Document with more occurrences should score higher
    let high_result = results.iter().find(|r| r.doc_id == high_freq).unwrap();
    let low_result = results.iter().find(|r| r.doc_id == low_freq).unwrap();
    assert!(high_result.score > low_result.score);
}

#[test]
fn test_search_after_document_removal() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "rust programming");
    index.add_document(doc2, "rust language");
    
    index.remove_document(doc1, "rust programming");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc2);
}

#[test]
fn test_search_hyphenated_words() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "state-of-the-art technology");
    
    let results = index.search("state-of-the-art");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_with_underscores() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "my_variable_name");
    
    let results = index.search("my_variable_name");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_phrase_search_case_insensitive() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "Rust Programming Language");
    
    let results = index.search_phrase("rust programming");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_returns_positions() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "word test word");
    
    let results = index.search("word");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].positions.len(), 2);
    assert!(results[0].positions.contains(&0));
    assert!(results[0].positions.contains(&2));
}

#[test]
fn test_phrase_search_score() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "test phrase");
    
    let results = index.search_phrase("test phrase");
    assert_eq!(results.len(), 1);
    // Phrase matches should have high scores
    assert!(results[0].score > 0.0);
}
