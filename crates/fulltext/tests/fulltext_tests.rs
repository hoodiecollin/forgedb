use forgedb_fulltext::*;
use uuid::Uuid;

#[test]
fn test_tokenizer_basic() {
    let tokens = Tokenizer::tokenize("Hello World");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}

#[test]
fn test_tokenizer_punctuation() {
    let tokens = Tokenizer::tokenize("Hello, World! How are you?");
    assert_eq!(tokens.len(), 5);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
    assert_eq!(tokens[2].text, "how");
    assert_eq!(tokens[3].text, "are");
    assert_eq!(tokens[4].text, "you");
}

#[test]
fn test_tokenizer_positions() {
    let tokens = Tokenizer::tokenize("one two three");
    assert_eq!(tokens[0].position, 0);
    assert_eq!(tokens[1].position, 1);
    assert_eq!(tokens[2].position, 2);
}

#[test]
fn test_trigrams() {
    let trigrams = Tokenizer::trigrams("hello");
    assert_eq!(trigrams, vec!["hel", "ell", "llo"]);
}

#[test]
fn test_trigrams_short() {
    let trigrams = Tokenizer::trigrams("hi");
    assert_eq!(trigrams, vec!["hi"]);
}

#[test]
fn test_index_add_and_search() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "The quick brown fox");
    index.add_document(doc2, "The lazy dog");

    let results = index.search("quick");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc1);
}

#[test]
fn test_index_multiple_matches() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "The quick brown fox");
    index.add_document(doc2, "The lazy dog");

    let results = index.search("the");
    assert_eq!(results.len(), 2);
}

#[test]
fn test_index_scoring() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "rust rust rust programming");
    index.add_document(doc2, "rust programming");

    let results = index.search("rust");
    assert_eq!(results.len(), 2);
    // doc1 should score higher because "rust" appears 3 times
    // Debug: print scores
    println!(
        "doc1 score: {}, doc2 score: {}",
        results[0].score, results[1].score
    );
    println!(
        "doc1 id: {:?}, doc2 id: {:?}",
        results[0].doc_id, results[1].doc_id
    );

    // Find which result is doc1
    let doc1_result = results.iter().find(|r| r.doc_id == doc1).unwrap();
    let doc2_result = results.iter().find(|r| r.doc_id == doc2).unwrap();

    assert!(
        doc1_result.score > doc2_result.score,
        "doc1 score ({}) should be > doc2 score ({})",
        doc1_result.score,
        doc2_result.score
    );
}

#[test]
fn test_phrase_search() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "the quick brown fox");
    index.add_document(doc2, "brown fox the quick");

    let results = index.search_phrase("quick brown");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, doc1);
}

#[test]
fn test_remove_document() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();

    index.add_document(doc1, "test document");
    assert_eq!(index.search("test").len(), 1);

    index.remove_document(doc1, "test document");
    assert_eq!(index.search("test").len(), 0);
}

#[test]
fn test_stats() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();

    index.add_document(doc1, "hello world");
    let stats = index.stats();

    assert_eq!(stats.total_docs, 1);
    assert_eq!(stats.total_terms, 2);
    assert!(stats.total_trigrams > 0);
}
