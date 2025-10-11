# TypeDB Storage Architecture

## Overview

TypeDB uses a hybrid columnar storage architecture optimized for different data types. The core principle is: **fixed-size data gets memory-mapped pages for zero-copy access, variable-length data gets append-only storage with offset indices**.

## Storage Layout

### On-Disk Structure

```
database/
├── manifest.json           # Schema version, row counts, metadata
├── schema.rs              # Generated schema (for reference)
├── tombstones.bin         # Deleted row bitmap
├── wal/                   # Write-ahead log
│   ├── 00000001.log
│   └── 00000002.log
│
├── fixed/                 # Fixed-size column storage
│   ├── u64.bin           # All u64 columns concatenated
│   ├── u32.bin           # All u32 columns
│   ├── i32.bin
│   ├── f64.bin
│   ├── bool.bin          # Bit-packed
│   ├── uuid.bin          # 16-byte UUIDs
│   ├── timestamp.bin     # 8-byte timestamps
│   ├── structs.bin       # All inline structs
│   └── ...
│
└── variable/              # Variable-length storage
    ├── string_data.bin    # Actual string bytes
    └── string_offsets.bin # u64 offsets into data file
```

## Columnar Storage

### Fixed-Size Columns

All columns of the same primitive type are stored in a single file, with each model's data in its own chunk.

#### Layout Example

```
// Schema:
User { id: u64, age: u32, ... }
Post { id: u64, views: u64, ... }

// Storage:
u64.bin = [
  // User.id chunk (offset: 0, count: 1000)
  user_id_0, user_id_1, ..., user_id_999,
  
  // Post.id chunk (offset: 8000, count: 5000)
  post_id_0, post_id_1, ..., post_id_4999,
  
  // Post.views chunk (offset: 48000, count: 5000)
  post_views_0, post_views_1, ..., post_views_4999
]
```

#### Memory Mapping

```rust
struct TypeFile {
    mmap: MmapMut,          // Memory-mapped file
    dirty_pages: BitVec,    // Track modified 4KB pages
}

// Direct access without deserialization
fn user_ids(&self) -> &[u64] {
    let offset = SCHEMA[USER_ID_INDEX].offset;
    let count = self.manifest.counts[USER_ID_INDEX];
    
    unsafe {
        slice::from_raw_parts(
            self.u64_file.mmap.as_ptr().add(offset) as *const u64,
            count
        )
    }
}
```

### Variable-Length Columns

Strings and other variable-length data use a two-part storage system:

1. **Data file**: Concatenated bytes
2. **Offset file**: Fixed-size (offset, length) pairs

#### String Storage

```
// Schema:
User { email: string, ... }

// Storage:
string_data.bin = [
  "alice@example.com\0bob@test.com\0charlie@demo.org\0..."
]

string_offsets.bin = [
  // User.email chunk
  (offset: 0,  length: 18),  // alice@example.com
  (offset: 18, length: 13),  // bob@test.com
  (offset: 31, length: 18),  // charlie@demo.org
  ...
]
```

#### Access Pattern

```rust
struct VarColumn {
    offsets: PagedColumn,     // Fixed-size (u64, u64) pairs
    data: AppendOnlyFile,     // Raw bytes
}

fn get_email(&self, index: usize) -> &str {
    let (offset, length) = self.email_offsets[index];
    let bytes = &self.email_data[offset..offset+length];
    std::str::from_utf8(bytes).unwrap()
}
```

### Inline Struct Storage

Fixed-size structs are stored directly in columnar format:

```
// Schema:
struct Location {
  lat: f64,
  lon: f64
}

User {
  id: u64,
  location: Location
}

// Storage:
structs.bin = [
  // User.location chunk (each 16 bytes)
  [lat0, lon0], [lat1, lon1], [lat2, lon2], ...
]
```

```rust
#[repr(C)]
struct Location {
    lat: f64,
    lon: f64,
}

fn user_locations(&self) -> &[Location] {
    unsafe {
        slice::from_raw_parts(
            self.structs_file.as_ptr().add(USER_LOCATION_OFFSET) as *const Location,
            self.user_count
        )
    }
}
```

### Fixed-Size Arrays in Structs

```
// Schema:
struct TimeSeries {
  values: [f64; 100]
  timestamps: [u64; 100]
}

Post {
  analytics: TimeSeries
}

// Storage: Each TimeSeries is 1600 bytes (800 + 800)
structs.bin = [
  // Post.analytics chunk
  TimeSeries { values: [..], timestamps: [..] },
  TimeSeries { values: [..], timestamps: [..] },
  ...
]
```

## Chunk Directory

The manifest tracks where each column lives:

```json
{
  "version": 1,
  "counts": {
    "User": 1000,
    "Post": 5000,
    "Comment": 15000
  },
  "chunks": {
    "User.id": {
      "type": "u64",
      "file": "fixed/u64.bin",
      "offset": 0,
      "element_size": 8,
      "count": 1000
    },
    "User.age": {
      "type": "u32",
      "file": "fixed/u32.bin",
      "offset": 0,
      "element_size": 4,
      "count": 1000
    },
    "User.email": {
      "type": "string",
      "offset_file": "variable/string_offsets.bin",
      "data_file": "variable/string_data.bin",
      "offset": 0,
      "count": 1000
    },
    "User.location": {
      "type": "struct:Location",
      "file": "fixed/structs.bin",
      "offset": 0,
      "element_size": 16,
      "count": 1000
    },
    "Post.id": {
      "type": "u64",
      "file": "fixed/u64.bin",
      "offset": 8000,
      "element_size": 8,
      "count": 5000
    }
  },
  "tombstones": {
    "User": { "offset": 0, "count": 1000 },
    "Post": { "offset": 125, "count": 5000 }
  }
}
```

## Tombstones

Deleted rows are marked in a bitmap rather than physically removed:

```
tombstones.bin = BitVec [
  // User tombstones (bits 0-999)
  0, 0, 1, 0, 0, ...  // User at index 2 is deleted
  
  // Post tombstones (bits 1000-5999)
  0, 0, 0, 1, ...
]
```

### Benefits
- O(1) deletion
- Recycling deleted slots for inserts
- Compaction can be done offline

### Compaction

Periodically, rewrite type files without tombstoned rows:

```rust
fn compact_model(&mut self, model: ModelId) {
    let tombstones = self.get_tombstones(model);
    
    // For each column in model
    for column in model.columns() {
        let old_data = self.read_column(column);
        let new_data = old_data.iter()
            .enumerate()
            .filter(|(i, _)| !tombstones[*i])
            .map(|(_, val)| val)
            .collect();
        
        self.write_column(column, new_data);
    }
    
    // Clear tombstones for this model
    self.clear_tombstones(model);
}
```

## Write Path

### Append-Only Writes (Default)

```rust
impl DB {
    fn insert_user(&mut self, email: String, age: u32) -> Result<uuid> {
        let id = uuid::Uuid::new_v4();
        let now = SystemTime::now();
        
        // Check for recycled slots
        let index = if let Some(slot) = self.find_deleted_slot(USER_MODEL) {
            self.tombstones.set(slot, false);
            slot
        } else {
            self.user_count
        };
        
        // Write to in-memory buffers
        self.user_ids[index] = id;
        self.user_emails.insert(index, email.clone());
        self.user_ages[index] = age;
        self.user_created_ats[index] = now;
        
        // Write to WAL
        self.wal.append(Write::Insert {
            model: USER_MODEL,
            index,
            data: serialize_user(&id, &email, &age, &now),
        });
        
        self.user_count += 1;
        Ok(id)
    }
}
```

### Update (In-Place for Fixed, Append for Variable)

```rust
impl DB {
    fn update_user(&mut self, id: uuid, email: Option<String>, age: Option<u32>) -> Result<()> {
        let index = self.find_user_index(id)?;
        
        // Fixed-size update (in-place)
        if let Some(new_age) = age {
            let offset = USER_AGE_OFFSET + (index * 4);
            let page = offset / PAGE_SIZE;
            
            self.u32_file.dirty_pages.set(page, true);
            self.user_ages[index] = new_age;
        }
        
        // Variable-length update (append)
        if let Some(new_email) = email {
            let data_offset = self.string_data_end.fetch_add(new_email.len() as u64);
            self.string_data.append(&new_email.as_bytes());
            
            // Update offset (in-place, it's fixed-size)
            self.user_email_offsets[index] = (data_offset, new_email.len() as u64);
        }
        
        // Update timestamp (in-place)
        self.user_updated_ats[index] = SystemTime::now();
        
        // WAL
        self.wal.append(Write::Update { ... });
        
        Ok(())
    }
}
```

### Delete (Tombstone)

```rust
impl DB {
    fn delete_user(&mut self, id: uuid) -> Result<()> {
        let index = self.find_user_index(id)?;
        
        // Set tombstone
        self.tombstones.set(USER_TOMBSTONE_OFFSET + index, true);
        
        // WAL
        self.wal.append(Write::Delete {
            model: USER_MODEL,
            index,
        });
        
        Ok(())
    }
}
```

## Write-Ahead Log (WAL)

For durability and crash recovery:

```
wal/
├── 00000001.log    # Active log
└── 00000002.log    # Previous log (kept for recovery)
```

### Log Entry Format

```rust
enum WriteOp {
    Insert { model: u16, index: usize, data: Vec<u8> },
    Update { model: u16, index: usize, fields: Vec<(u16, Vec<u8>)> },
    Delete { model: u16, index: usize },
    Commit { transaction_id: u64 },
}
```

### Commit Flow

```rust
fn commit(&mut self) -> Result<()> {
    // 1. Write commit record to WAL
    self.wal.append(WriteOp::Commit { transaction_id: self.tx_id });
    self.wal.sync()?;
    
    // 2. Flush dirty pages to disk
    for file in &mut self.fixed_files {
        file.flush_dirty_pages()?;
    }
    
    // 3. Sync variable-length data
    for file in &mut self.variable_files {
        file.sync()?;
    }
    
    // 4. Update manifest
    self.manifest.counts = self.current_counts();
    self.write_manifest()?;
    
    // 5. Mark WAL checkpoint
    self.wal.checkpoint()?;
    
    Ok(())
}
```

### Recovery

```rust
fn recover() -> Result<DB> {
    let manifest = read_manifest()?;
    let db = DB::from_manifest(manifest)?;
    
    // Replay WAL from last checkpoint
    for entry in read_wal_since_checkpoint()? {
        match entry {
            WriteOp::Insert { model, index, data } => {
                db.replay_insert(model, index, data)?;
            }
            WriteOp::Update { model, index, fields } => {
                db.replay_update(model, index, fields)?;
            }
            WriteOp::Delete { model, index } => {
                db.replay_delete(model, index)?;
            }
            WriteOp::Commit { .. } => {
                // Transaction boundary
            }
        }
    }
    
    Ok(db)
}
```

## Query Execution

### Columnar Scanning

```rust
// Filter: age > 25 AND email contains "@gmail"
fn query_users(&self) -> Vec<usize> {
    // Step 1: Scan age column (sequential, cache-friendly)
    let age_matches = self.user_ages
        .iter()
        .enumerate()
        .filter(|(_, &age)| age > 25)
        .map(|(i, _)| i)
        .collect::<BitVec>();
    
    // Step 2: Scan email offsets only for age matches
    let mut email_matches = BitVec::repeat(false, self.user_count);
    for i in age_matches.iter_ones() {
        let email = self.get_user_email(i);
        if email.contains("@gmail") {
            email_matches.set(i, true);
        }
    }
    
    // Step 3: Combine with tombstones
    let final_matches = age_matches & email_matches & !self.user_tombstones();
    
    // Step 4: Return matching indices
    final_matches.iter_ones().collect()
}
```

### Vectorized Operations

For numeric predicates, use SIMD:

```rust
use std::simd::*;

fn filter_ages_vectorized(&self, threshold: u32) -> BitVec {
    let ages = self.user_ages.as_slice();
    let mut result = BitVec::repeat(false, ages.len());
    
    let threshold_vec = u32x8::splat(threshold);
    
    // Process 8 elements at once
    for (i, chunk) in ages.chunks_exact(8).enumerate() {
        let age_vec = u32x8::from_slice(chunk);
        let mask = age_vec.simd_gt(threshold_vec);
        
        for j in 0..8 {
            if mask.test(j) {
                result.set(i * 8 + j, true);
            }
        }
    }
    
    // Handle remainder
    // ...
    
    result
}
```

### Projection (Select Specific Columns)

Only load needed columns:

```rust
// SELECT id, email FROM users WHERE age > 25

let matching_indices = self.filter_by_age(25);

// Only touch id and email columns
let results: Vec<(uuid, String)> = matching_indices
    .iter()
    .map(|&i| {
        (
            self.user_ids[i],
            self.get_user_email(i).to_owned()
        )
    })
    .collect();
```

### Joins

```rust
// SELECT * FROM posts WHERE user.email = "alice@example.com"

// Step 1: Find user
let user_idx = self.find_user_by_email("alice@example.com")?;
let user_id = self.user_ids[user_idx];

// Step 2: Scan Post.user_id column
let post_indices: Vec<usize> = self.post_user_ids
    .iter()
    .enumerate()
    .filter(|(_, &uid)| uid == user_id)
    .map(|(i, _)| i)
    .collect();

// Step 3: Materialize posts
let posts = self.load_posts(&post_indices);
```

## Indexing

### Hash Index (for unique/equality lookups)

```rust
struct HashIndex<K, V> {
    map: HashMap<K, V>,
}

// User.email index
let email_index: HashMap<String, usize> = self.user_emails
    .iter()
    .enumerate()
    .map(|(i, email)| (email.clone(), i))
    .collect();

// Lookup: O(1)
let index = email_index.get("alice@example.com")?;
```

### B-Tree Index (for range queries)

```rust
use std::collections::BTreeMap;

// User.age index
let age_index: BTreeMap<u32, Vec<usize>> = ...;

// Range query: 25 <= age <= 65
let matching_indices: Vec<usize> = age_index
    .range(25..=65)
    .flat_map(|(_, indices)| indices.iter().copied())
    .collect();
```

### Full-Text Index (for string search)

```rust
// Inverted index: term -> [doc indices]
struct FullTextIndex {
    terms: HashMap<String, Vec<usize>>,
}

// Post.content full-text search
fn search_posts(&self, query: &str) -> Vec<usize> {
    let terms = tokenize(query);
    
    // Get documents containing all terms
    let mut results = self.fulltext_index.get(&terms[0]).cloned().unwrap_or_default();
    
    for term in &terms[1..] {
        let term_docs = self.fulltext_index.get(term).cloned().unwrap_or_default();
        results.retain(|idx| term_docs.contains(idx));
    }
    
    results
}
```

## Performance Characteristics

### Memory Usage

**Fixed-size columns:**
- Mapped directly to memory
- OS manages paging
- Memory usage = (row_count * element_size)

**Variable-length columns:**
- Offsets: (row_count * 16 bytes) [mapped]
- Data: sum(string_lengths) [can be partially mapped]

**Indexes:**
- Hash index: ~(row_count * (key_size + 8))
- B-Tree: ~(row_count * (key_size + 8)) with overhead
- Full-text: Depends on corpus and tokenization

### Query Performance

**Scan operations:**
- Sequential access: ~10-20 GB/s (memory bandwidth)
- SIMD acceleration: 4-8x for numeric predicates
- Cache-friendly: columnar layout minimizes cache misses

**Index lookups:**
- Hash: O(1) average, ~100ns
- B-Tree: O(log n), ~200-500ns
- Full-text: O(k * log n) where k = number of terms

**Joins:**
- Hash join: O(n + m)
- Index lookup join: O(n * log m)

### Write Performance

**Inserts:**
- Append to in-memory buffers: ~1-2 μs
- WAL write: ~5-10 μs (depending on fsync policy)
- Batch inserts: ~100k/sec single-threaded

**Updates:**
- Fixed-size in-place: ~1-2 μs
- Variable-length append: ~2-5 μs
- WAL write: ~5-10 μs

**Deletes:**
- Tombstone set: ~500ns
- WAL write: ~5-10 μs

## Compaction Strategy

### When to Compact

Trigger compaction when:
- Tombstone ratio > 20%
- String data file has too much dead space
- Manual trigger

### Compaction Process

```rust
fn compact(&mut self) -> Result<()> {
    // 1. Build mapping of old -> new indices
    let mut index_map = Vec::new();
    let mut new_index = 0;
    
    for old_index in 0..self.row_count {
        if !self.is_deleted(old_index) {
            index_map.push(Some(new_index));
            new_index += 1;
        } else {
            index_map.push(None);
        }
    }
    
    // 2. Rewrite fixed-size columns
    for column in self.fixed_columns() {
        let old_data = self.read_column(column);
        let new_data: Vec<_> = index_map
            .iter()
            .enumerate()
            .filter_map(|(old_idx, new_idx)| {
                new_idx.map(|_| old_data[old_idx])
            })
            .collect();
        
        self.write_column(column, &new_data)?;
    }
    
    // 3. Rewrite variable-length columns
    for column in self.variable_columns() {
        let mut new_data = Vec::new();
        let mut new_offsets = Vec::new();
        let mut current_offset = 0u64;
        
        for (old_idx, new_idx) in index_map.iter().enumerate() {
            if new_idx.is_some() {
                let string = self.get_string(column, old_idx);
                new_offsets.push((current_offset, string.len() as u64));
                new_data.extend_from_slice(string.as_bytes());
                current_offset += string.len() as u64;
            }
        }
        
        self.write_variable_column(column, &new_data, &new_offsets)?;
    }
    
    // 4. Update indices
    for index in self.indices_mut() {
        index.remap(&index_map)?;
    }
    
    // 5. Clear tombstones
    self.tombstones.clear();
    
    // 6. Update manifest
    self.row_count = new_index;
    self.write_manifest()?;
    
    Ok(())
}
```

## Concurrency

### Read Concurrency

Multiple readers can access memory-mapped files concurrently:

```rust
use std::sync::Arc;

struct DB {
    // Shared immutable access to mmap'd files
    fixed_files: Arc<FixedFiles>,
    variable_files: Arc<VariableFiles>,
    
    // ...
}

impl DB {
    fn query_parallel(&self) -> Vec<Result> {
        use rayon::prelude::*;
        
        (0..self.row_count)
            .into_par_iter()
            .filter(|&i| !self.is_deleted(i))
            .map(|i| self.load_row(i))
            .collect()
    }
}
```

### Write Concurrency

Single-writer model (for v1):

```rust
use std::sync::RwLock;

struct DB {
    data: RwLock<DBData>,
}

impl DB {
    fn insert(&self, ...) -> Result<()> {
        let mut data = self.data.write().unwrap();
        data.insert_internal(...)?;
        Ok(())
    }
    
    fn query(&self, ...) -> Result<Vec<Row>> {
        let data = self.data.read().unwrap();
        data.query_internal(...)
    }
}
```

### Future: MVCC (Multi-Version Concurrency Control)

For v2, implement versioned rows:

```rust
struct Row {
    data: RowData,
    version: u64,
    created_at: TransactionId,
    deleted_at: Option<TransactionId>,
}

// Readers see consistent snapshot
fn query_at_version(&self, version: u64) -> Vec<Row> {
    self.rows
        .iter()
        .filter(|row| {
            row.created_at <= version &&
            row.deleted_at.map_or(true, |v| v > version)
        })
        .collect()
}
```

## Benchmarking Goals

### Target Performance (on commodity hardware)

**Sequential scans:**
- Numeric predicates: 1-5 GB/s per core
- String predicates: 500 MB/s - 2 GB/s per core

**Indexed lookups:**
- Point queries: < 1 μs (hot cache)
- Range queries: ~100-500 ns per result

**Inserts:**
- Single: < 10 μs (including WAL)
- Batch (10k): < 100 ms

**Full CRUD cycle:**
- Insert, update, query, delete: < 50 μs total

---

**Document Version**: 0.1.0
**Last Updated**: 2025-10-11
**Status**: Architecture Specification
