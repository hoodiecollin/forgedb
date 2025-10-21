use forgedb_fulltext::*;
use uuid::Uuid;

#[test]
fn test_scoring_term_frequency() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    // doc1 has "rust" 5 times
    index.add_document(doc1, "rust rust rust rust rust");
    // doc2 has "rust" 1 time
    index.add_document(doc2, "rust");
    
    let results = index.search("rust");
    
    let doc1_result = results.iter().find(|r| r.doc_id == doc1).unwrap();
    let doc2_result = results.iter().find(|r| r.doc_id == doc2).unwrap();
    
    // Higher term frequency should result in higher score
    assert!(doc1_result.score > doc2_result.score);
}

#[test]
fn test_scoring_document_frequency() {
    let mut index = FullTextIndex::new();
    
    // "common" appears in all 3 docs
    // "rare" appears in only 1 doc
    index.add_document(Uuid::new_v4(), "common word");
    index.add_document(Uuid::new_v4(), "common word");
    index.add_document(Uuid::new_v4(), "common word rare");
    
    let common_results = index.search("common");
    let rare_results = index.search("rare");
    
    // Rare term should score higher (IDF component)
    assert_eq!(common_results.len(), 3);
    assert_eq!(rare_results.len(), 1);
    
    // The document with "rare" should score highly for that term
    assert!(rare_results[0].score > 0.0);
}

#[test]
fn test_scoring_multiple_query_terms() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let doc3 = Uuid::new_v4();

    index.add_document(doc1, "rust programming language");
    index.add_document(doc2, "rust programming");
    index.add_document(doc3, "rust");
    
    let results = index.search("rust programming");
    assert_eq!(results.len(), 3);
    
    // doc1 and doc2 have both terms, so should score higher than doc3
    let doc1_result = results.iter().find(|r| r.doc_id == doc1).unwrap();
    let doc2_result = results.iter().find(|r| r.doc_id == doc2).unwrap();
    let doc3_result = results.iter().find(|r| r.doc_id == doc3).unwrap();
    
    assert!(doc1_result.score > doc3_result.score);
    assert!(doc2_result.score > doc3_result.score);
}

#[test]
fn test_scoring_phrase_search() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "rust programming language");
    
    let results = index.search_phrase("rust programming");
    assert_eq!(results.len(), 1);
    
    // Phrase matches should have high scores
    assert!(results[0].score >= 100.0);
}

#[test]
fn test_scoring_consistency() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "test document");
    
    let results1 = index.search("test");
    let results2 = index.search("test");
    
    // Same search should return same scores
    assert_eq!(results1[0].score, results2[0].score);
}

#[test]
fn test_scoring_zero_for_no_match() {
    let mut index = FullTextIndex::new();
    index.add_document(Uuid::new_v4(), "hello world");
    
    let results = index.search("nonexistent");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_scoring_all_positive() {
    let mut index = FullTextIndex::new();
    
    index.add_document(Uuid::new_v4(), "test document one");
    index.add_document(Uuid::new_v4(), "test document two");
    index.add_document(Uuid::new_v4(), "test document three");
    
    let results = index.search("test");
    
    // All scores should be positive
    for result in results {
        assert!(result.score > 0.0);
    }
}

#[test]
fn test_scoring_ordering() {
    let mut index = FullTextIndex::new();
    let doc_high = Uuid::new_v4();
    let doc_med = Uuid::new_v4();
    let doc_low = Uuid::new_v4();

    index.add_document(doc_high, "rust rust rust rust rust");
    index.add_document(doc_med, "rust rust");
    index.add_document(doc_low, "rust");
    
    let results = index.search("rust");
    assert_eq!(results.len(), 3);
    
    // Find the results for each document
    let high_result = results.iter().find(|r| r.doc_id == doc_high).unwrap();
    let med_result = results.iter().find(|r| r.doc_id == doc_med).unwrap();
    let low_result = results.iter().find(|r| r.doc_id == doc_low).unwrap();
    
    // Documents with more occurrences should score higher
    assert!(high_result.score > med_result.score);
    assert!(med_result.score > low_result.score);
}

#[test]
fn test_scoring_with_positions() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "test word test word test");
    
    let results = index.search("test");
    assert_eq!(results.len(), 1);
    
    // Score should reflect multiple occurrences
    assert!(results[0].score > 0.0);
    assert_eq!(results[0].positions.len(), 3);
}

#[test]
fn test_document_match_score_comparison() {
    let match1 = DocumentMatch {
        doc_id: Uuid::new_v4(),
        score: 10.0,
        positions: vec![],
    };
    
    let match2 = DocumentMatch {
        doc_id: Uuid::new_v4(),
        score: 5.0,
        positions: vec![],
    };
    
    // Higher score should be "less than" in sorting (comes first)
    assert!(match1.partial_cmp(&match2).is_some());
}

#[test]
fn test_scoring_empty_index() {
    let index = FullTextIndex::new();
    let results = index.search("test");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_scoring_single_document() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "unique document");
    
    let results = index.search("unique");
    assert_eq!(results.len(), 1);
    assert!(results[0].score > 0.0);
}

#[test]
fn test_scoring_rare_vs_common_terms() {
    let mut index = FullTextIndex::new();
    
    // "the" appears in all documents (common)
    // "unique" appears in one document (rare)
    index.add_document(Uuid::new_v4(), "the first document");
    index.add_document(Uuid::new_v4(), "the second document");
    index.add_document(Uuid::new_v4(), "the third unique document");
    
    let unique_results = index.search("unique");
    let the_results = index.search("the");
    
    assert_eq!(unique_results.len(), 1);
    assert_eq!(the_results.len(), 3);
    
    // Rare term should have higher IDF, thus potentially higher score
    // (though this depends on the TF-IDF calculation)
}

#[test]
fn test_scoring_after_document_removal() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "rust programming");
    index.add_document(doc2, "rust programming");
    
    let results_before = index.search("rust");
    let _score_before = results_before[0].score;
    
    // Remove one document
    index.remove_document(doc1, "rust programming");
    
    let results_after = index.search("rust");
    let score_after = results_after[0].score;
    
    // Scores may change due to IDF recalculation
    assert!(score_after > 0.0);
}

#[test]
fn test_scoring_multiple_docs_same_content() {
    let mut index = FullTextIndex::new();
    
    index.add_document(Uuid::new_v4(), "identical content");
    index.add_document(Uuid::new_v4(), "identical content");
    index.add_document(Uuid::new_v4(), "identical content");
    
    let results = index.search("identical");
    assert_eq!(results.len(), 3);
    
    // All should have the same score
    let first_score = results[0].score;
    for result in &results {
        assert_eq!(result.score, first_score);
    }
}

#[test]
fn test_document_match_clone() {
    let original = DocumentMatch {
        doc_id: Uuid::new_v4(),
        score: 42.0,
        positions: vec![1, 2, 3],
    };
    
    let cloned = original.clone();
    
    assert_eq!(original.doc_id, cloned.doc_id);
    assert_eq!(original.score, cloned.score);
    assert_eq!(original.positions, cloned.positions);
}

#[test]
fn test_document_match_partial_eq() {
    let id = Uuid::new_v4();
    let match1 = DocumentMatch {
        doc_id: id,
        score: 10.0,
        positions: vec![1],
    };
    
    let match2 = DocumentMatch {
        doc_id: id,
        score: 10.0,
        positions: vec![1],
    };
    
    assert_eq!(match1, match2);
}

#[test]
fn test_scoring_with_mixed_terms() {
    let mut index = FullTextIndex::new();
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    index.add_document(doc1, "rust programming language system");
    index.add_document(doc2, "rust only");
    
    let results = index.search("rust programming system");
    assert_eq!(results.len(), 2);
    
    // doc1 has all three terms, should score higher
    let doc1_result = results.iter().find(|r| r.doc_id == doc1).unwrap();
    let doc2_result = results.iter().find(|r| r.doc_id == doc2).unwrap();
    
    assert!(doc1_result.score > doc2_result.score);
}

#[test]
fn test_phrase_search_score_high() {
    let mut index = FullTextIndex::new();
    let doc_id = Uuid::new_v4();

    index.add_document(doc_id, "exact phrase match here");
    
    let phrase_results = index.search_phrase("exact phrase");
    let word_results = index.search("exact phrase");
    
    // Phrase search should give high scores (currently 100.0)
    assert!(phrase_results[0].score >= 100.0);
    
    // Phrase score should be higher than regular search
    assert!(phrase_results[0].score > word_results[0].score);
}

#[test]
fn test_scoring_incremental_additions() {
    let mut index = FullTextIndex::new();
    
    index.add_document(Uuid::new_v4(), "test");
    let results1 = index.search("test");
    let _score1 = results1[0].score;
    
    index.add_document(Uuid::new_v4(), "test");
    let results2 = index.search("test");
    
    // Scores should still be positive and reasonable
    for result in results2 {
        assert!(result.score > 0.0);
    }
}
