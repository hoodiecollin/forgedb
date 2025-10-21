# ForgeDB Full-Text Search

High-performance full-text search engine with TF-IDF scoring and advanced query capabilities.

## Overview

The `forgedb-fulltext` crate provides a full-text search engine built around an inverted index. It supports tokenization, normalization, trigram-based substring matching, TF-IDF scoring for relevance ranking, and advanced query features like phrase search and boolean operations.

## Features

- **Inverted Index** - Efficient document lookup by term with position tracking
- **Tokenization and Normalization** - Converts text to lowercase, removes punctuation, splits on whitespace
- **Trigram-based Indexing** - Enables fuzzy and substring matching using character trigrams
- **TF-IDF Scoring** - Ranks search results by term frequency and inverse document frequency
- **Boolean Operators** - Support for AND, OR, NOT operations (implicit through search)
- **Phrase Search** - Find exact phrase matches with position-aware matching
- **Document Management** - Add and remove documents dynamically
- **Index Statistics** - Track total documents, terms, and trigrams

## Usage

### Basic Example

```rust
use forgedb_fulltext::FullTextIndex;
use uuid::Uuid;

fn main() {
    // Create a new search index
    let mut index = FullTextIndex::new();
    
    // Add documents
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let doc3 = Uuid::new_v4();
    
    index.add_document(doc1, "The quick brown fox jumps over the lazy dog");
    index.add_document(doc2, "A lazy dog sleeps in the sun");
    index.add_document(doc3, "The quick rabbit hops quickly");
    
    // Search for documents
    let results = index.search("lazy dog");
    
    for result in results {
        println!("Document: {:?}, Score: {:.2}", result.doc_id, result.score);
    }
}
```

### Adding Documents

Documents are identified by UUID and indexed by their text content:

```rust
use forgedb_fulltext::FullTextIndex;
use uuid::Uuid;

let mut index = FullTextIndex::new();
let doc_id = Uuid::new_v4();

index.add_document(doc_id, "Rust is a systems programming language");
```

### Searching with Queries

Simple search returns documents ranked by relevance:

```rust
// Search returns DocumentMatch with score and positions
let results = index.search("rust programming");

for result in &results {
    println!("Score: {:.2}, Positions: {:?}", result.score, result.positions);
}
```

### Phrase Search

Find exact phrase matches:

```rust
// Only matches documents with "quick brown" in that exact order
let results = index.search_phrase("quick brown");

for result in &results {
    println!("Document: {:?}, Positions: {:?}", result.doc_id, result.positions);
}
```

### Ranking Results

Results are automatically sorted by relevance score (highest first):

```rust
let results = index.search("rust");

// Results are already sorted by score (descending)
if let Some(best_match) = results.first() {
    println!("Best match: {:?} with score {:.2}", best_match.doc_id, best_match.score);
}
```

### Removing Documents

Remove documents from the index:

```rust
// Must provide the same text used when adding
index.remove_document(doc_id, "Rust is a systems programming language");
```

### Index Statistics

Get information about the index:

```rust
let stats = index.stats();
println!("Total documents: {}", stats.total_docs);
println!("Total terms: {}", stats.total_terms);
println!("Total trigrams: {}", stats.total_trigrams);
```

## Query Syntax

### Simple Queries

Single word or multiple word queries:

```rust
index.search("rust");           // Single term
index.search("rust programming"); // Multiple terms (OR operation)
```

### Boolean Operations

While explicit boolean operators are not yet implemented in the query syntax, the search behavior provides:

- **OR (implicit)** - `index.search("term1 term2")` matches documents containing either term
- **AND (implicit)** - Documents with multiple matching terms score higher
- **NOT** - Can be implemented by filtering results

### Phrase Search

Search for exact phrases using `search_phrase`:

```rust
index.search_phrase("systems programming language");
```

This matches only documents where these words appear consecutively in order.

### Substring Matching

Trigram indexing enables fuzzy substring matching (currently internal, future enhancement for explicit fuzzy queries).

## API Reference

### Types

#### `FullTextIndex`

The main search index structure.

**Methods:**
- `new() -> Self` - Create a new empty index
- `add_document(doc_id: Uuid, text: &str)` - Add a document to the index
- `remove_document(doc_id: Uuid, text: &str)` - Remove a document from the index
- `search(query: &str) -> Vec<DocumentMatch>` - Search for documents matching the query
- `search_phrase(phrase: &str) -> Vec<DocumentMatch>` - Search for exact phrase matches
- `stats() -> IndexStats` - Get index statistics

#### `Token`

Represents a tokenized word with position information.

**Fields:**
- `text: String` - The normalized token text
- `position: usize` - Position in the original text

#### `DocumentMatch`

Represents a search result with scoring information.

**Fields:**
- `doc_id: Uuid` - The document identifier
- `score: f64` - Relevance score (higher is better)
- `positions: Vec<usize>` - Positions where matches occur in the document

#### `IndexStats`

Statistics about the index.

**Fields:**
- `total_docs: usize` - Total number of indexed documents
- `total_terms: usize` - Total number of unique terms
- `total_trigrams: usize` - Total number of trigrams in the index

### Tokenizer

Static tokenization utilities:

- `Tokenizer::tokenize(text: &str) -> Vec<Token>` - Tokenize and normalize text
- `Tokenizer::trigrams(word: &str) -> Vec<String>` - Generate trigrams from a word

## Scoring

### TF-IDF (Term Frequency - Inverse Document Frequency)

The search engine uses TF-IDF scoring to rank results by relevance:

**Term Frequency (TF)**: How many times a term appears in a document. More occurrences = higher score.

**Inverse Document Frequency (IDF)**: How rare a term is across all documents. Rare terms = higher score.

**Formula:**
```
score = TF * IDF

where:
  TF = count of term in document
  IDF = ln((total_docs + 1) / (docs_containing_term + 1)) + 1
```

**Example:**
- A document containing "rust" 3 times will score higher than one containing it once
- A document containing the rare term "tokio" will score higher than one with the common term "the"

### Relevance Ranking

Documents are ranked by their total score across all query terms:

1. For each query term, calculate TF-IDF score for each document
2. Sum scores for all query terms per document
3. Sort documents by total score (descending)

**Phrase search** uses a fixed high score (100.0) for exact matches since finding consecutive terms is more significant than TF-IDF.

## Performance

### Index Size

- **Terms**: O(unique_terms) - One entry per unique term
- **Trigrams**: O(unique_trigrams) - Approximately 3 * unique_terms for words > 3 characters
- **Positions**: O(total_tokens) - All token positions stored for phrase matching

### Query Speed

- **Simple Search**: O(query_terms * docs_per_term) - Lookup is fast via HashMap
- **Phrase Search**: O(docs_with_first_term * phrase_length) - Must verify consecutive positions
- **Sorting**: O(n log n) where n = number of matching documents

### Memory Considerations

The index stores:
- All unique terms
- Document positions for each term occurrence
- Trigram mappings for substring matching
- Document frequency counts

For large document collections, consider:
- Limiting position storage
- Pruning rare trigrams
- Implementing index persistence

## Testing

Run the test suite:

```bash
# Run all tests
cargo test -p forgedb-fulltext

# Run with output
cargo test -p forgedb-fulltext -- --nocapture

# Run specific test
cargo test -p forgedb-fulltext test_index_scoring
```

### Test Coverage

The crate includes comprehensive tests for:

- ✅ **Tokenization** - Basic tokenization, punctuation handling, position tracking
- ✅ **Trigrams** - Trigram generation for various word lengths
- ✅ **Indexing** - Adding documents, searching, multiple matches
- ✅ **Scoring** - TF-IDF ranking verification
- ✅ **Phrase Search** - Exact phrase matching with position awareness
- ✅ **Document Management** - Adding and removing documents
- ✅ **Statistics** - Index size tracking

## Dependencies

- `uuid` (v1.6) - Document identification
- `regex` (v1.10) - Pattern matching support
- `unicode-segmentation` (v1.10) - Unicode-aware text processing

## Future Enhancements

- Explicit boolean operators in query syntax (AND, OR, NOT)
- Fuzzy search with edit distance
- Wildcard and prefix matching
- Configurable tokenization rules
- Stop word filtering
- Stemming support
- Index persistence and serialization
- Incremental index updates
- Query result highlighting

## Documentation

For more information about ForgeDB:

- **[ForgeDB Architecture](../../docs/ARCHITECTURE.md)** - System design and component architecture
- **[Public Crates Guide](../../docs/PUBLIC_CRATES.md)** - Complete runtime library documentation
- **[Development Guide](../../docs/DEVELOPMENT.md)** - Development setup and workflow

## License

Part of the ForgeDB project.
