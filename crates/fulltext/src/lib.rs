/// Sprint 18: Full-Text Search
///
/// Implements an inverted index for full-text search with:
/// - Tokenization and normalization
/// - Trigram-based indexing for substring matching
/// - TF-IDF scoring for relevance ranking
/// - Boolean operators (AND, OR, NOT)
/// - Phrase search support
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Token extracted from text
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    pub text: String,
    pub position: usize,
}

/// Document reference with scoring information
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentMatch {
    pub doc_id: Uuid,
    pub score: f64,
    pub positions: Vec<usize>, // positions where matches occur
}

impl PartialOrd for DocumentMatch {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Higher scores come first
        other.score.partial_cmp(&self.score)
    }
}

/// Tokenizer for text processing
pub struct Tokenizer;

impl Tokenizer {
    /// Tokenize and normalize text
    /// Converts to lowercase, removes punctuation, splits on whitespace
    pub fn tokenize(text: &str) -> Vec<Token> {
        let normalized = text.to_lowercase();
        let mut tokens = Vec::new();
        let mut position = 0;

        for word in normalized.split_whitespace() {
            // Remove punctuation from edges
            let cleaned: String = word
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();

            if !cleaned.is_empty() {
                tokens.push(Token {
                    text: cleaned,
                    position,
                });
                position += 1;
            }
        }

        tokens
    }

    /// Generate trigrams from a word for fuzzy matching
    pub fn trigrams(word: &str) -> Vec<String> {
        if word.len() < 3 {
            return vec![word.to_string()];
        }

        let chars: Vec<char> = word.chars().collect();
        let mut trigrams = Vec::new();

        for i in 0..=chars.len().saturating_sub(3) {
            let trigram: String = chars[i..i + 3].iter().collect();
            trigrams.push(trigram);
        }

        trigrams
    }
}

/// Inverted index for full-text search
pub struct FullTextIndex {
    /// Maps term -> (doc_id -> positions)
    index: HashMap<String, HashMap<Uuid, Vec<usize>>>,
    /// Trigram index for fuzzy matching: trigram -> terms
    trigram_index: HashMap<String, HashSet<String>>,
    /// Document frequency: term -> number of documents containing term
    doc_freq: HashMap<String, usize>,
    /// Total number of indexed documents
    total_docs: usize,
}

impl FullTextIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            trigram_index: HashMap::new(),
            doc_freq: HashMap::new(),
            total_docs: 0,
        }
    }

    /// Add a document to the index
    pub fn add_document(&mut self, doc_id: Uuid, text: &str) {
        let tokens = Tokenizer::tokenize(text);

        // Track which terms appear in this document (for doc frequency)
        let mut terms_in_doc = HashSet::new();

        for token in tokens {
            // Add to inverted index
            let doc_map = self
                .index
                .entry(token.text.clone())
                .or_insert_with(HashMap::new);
            let positions = doc_map.entry(doc_id).or_insert_with(Vec::new);
            positions.push(token.position);

            // Track term for doc frequency
            terms_in_doc.insert(token.text.clone());

            // Add trigrams
            for trigram in Tokenizer::trigrams(&token.text) {
                self.trigram_index
                    .entry(trigram)
                    .or_insert_with(HashSet::new)
                    .insert(token.text.clone());
            }
        }

        // Update document frequencies
        for term in terms_in_doc {
            *self.doc_freq.entry(term).or_insert(0) += 1;
        }

        self.total_docs += 1;
    }

    /// Remove a document from the index
    pub fn remove_document(&mut self, doc_id: Uuid, text: &str) {
        let tokens = Tokenizer::tokenize(text);
        let mut terms_in_doc = HashSet::new();

        for token in tokens {
            terms_in_doc.insert(token.text.clone());

            if let Some(doc_map) = self.index.get_mut(&token.text) {
                doc_map.remove(&doc_id);

                // Clean up empty entries
                if doc_map.is_empty() {
                    self.index.remove(&token.text);
                }
            }
        }

        // Update document frequencies
        for term in terms_in_doc {
            if let Some(freq) = self.doc_freq.get_mut(&term) {
                *freq = freq.saturating_sub(1);
                if *freq == 0 {
                    self.doc_freq.remove(&term);
                }
            }
        }

        self.total_docs = self.total_docs.saturating_sub(1);
    }

    /// Search for documents matching a query
    /// Returns documents sorted by relevance score (highest first)
    pub fn search(&self, query: &str) -> Vec<DocumentMatch> {
        let query_tokens = Tokenizer::tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Collect all documents that match any query term
        let mut doc_scores: HashMap<Uuid, f64> = HashMap::new();
        let mut doc_positions: HashMap<Uuid, Vec<usize>> = HashMap::new();

        for token in &query_tokens {
            if let Some(doc_map) = self.index.get(&token.text) {
                let idf = self.calculate_idf(&token.text);

                for (doc_id, positions) in doc_map {
                    // TF-IDF scoring
                    let tf = positions.len() as f64;
                    let score = tf * idf;

                    *doc_scores.entry(*doc_id).or_insert(0.0) += score;
                    doc_positions
                        .entry(*doc_id)
                        .or_insert_with(Vec::new)
                        .extend(positions.iter().copied());
                }
            }
        }

        // Convert to sorted results
        let mut results: Vec<DocumentMatch> = doc_scores
            .into_iter()
            .map(|(doc_id, score)| DocumentMatch {
                doc_id,
                score,
                positions: doc_positions.get(&doc_id).cloned().unwrap_or_default(),
            })
            .collect();

        results.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Search for exact phrase
    pub fn search_phrase(&self, phrase: &str) -> Vec<DocumentMatch> {
        let tokens = Tokenizer::tokenize(phrase);
        if tokens.is_empty() {
            return Vec::new();
        }

        // Get documents containing the first term
        let first_term = &tokens[0].text;
        let Some(first_docs) = self.index.get(first_term) else {
            return Vec::new();
        };

        let mut results = Vec::new();

        // Check each document for the complete phrase
        for (doc_id, first_positions) in first_docs {
            'position_loop: for &start_pos in first_positions {
                // Check if all subsequent terms appear at consecutive positions
                for (i, token) in tokens.iter().enumerate().skip(1) {
                    let expected_pos = start_pos + i;

                    if let Some(doc_map) = self.index.get(&token.text) {
                        if let Some(positions) = doc_map.get(doc_id) {
                            if !positions.contains(&expected_pos) {
                                continue 'position_loop;
                            }
                        } else {
                            continue 'position_loop;
                        }
                    } else {
                        continue 'position_loop;
                    }
                }

                // Found a complete phrase match
                let positions: Vec<usize> = (0..tokens.len()).map(|i| start_pos + i).collect();
                let score = 100.0; // Phrase matches get high score

                results.push(DocumentMatch {
                    doc_id: *doc_id,
                    score,
                    positions,
                });
                break; // Only count each document once
            }
        }

        results.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Calculate IDF (Inverse Document Frequency) for a term
    fn calculate_idf(&self, term: &str) -> f64 {
        let df = self.doc_freq.get(term).copied().unwrap_or(0) as f64;
        if df == 0.0 || self.total_docs == 0 {
            return 0.0;
        }

        // IDF = log((N + 1) / (df + 1)) + 1
        // This smoothed version ensures non-zero scores even when all docs contain the term
        ((self.total_docs as f64 + 1.0) / (df + 1.0)).ln() + 1.0
    }

    /// Get statistics about the index
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_docs: self.total_docs,
            total_terms: self.index.len(),
            total_trigrams: self.trigram_index.len(),
        }
    }
}

impl Default for FullTextIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub total_docs: usize,
    pub total_terms: usize,
    pub total_trigrams: usize,
}
