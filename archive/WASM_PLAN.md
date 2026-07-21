# WASM Support: Detailed Implementation Plan

**Priority**: High
**Effort Level**: 🟢 Low
**Estimated Time**: 2-3 weeks
**Created**: 2025-10-15

---

## Overview

Compile ForgeDB core database engine to WebAssembly for browser deployment. Focus on **database functionality only** - no UI rendering, API generation, or server-side features.

### In Scope
- ✅ Core database operations (CRUD)
- ✅ Query engine
- ✅ Indexing and search
- ✅ Schema parsing and validation
- ✅ In-memory storage (defer IndexedDB)
- ✅ JavaScript/TypeScript bindings

### Out of Scope
- ❌ REST API generation (server-side only)
- ❌ UI component rendering (server-side only)
- ❌ Route handlers (server-side only)
- ❌ LSP/IDE features (development tools only)
- ❌ IndexedDB persistence (deferred to future)

---

## Architecture

### WASM Module Structure

```
forgedb-wasm/
├── src/
│   ├── lib.rs              # Main WASM entry point
│   ├── memory.rs           # In-memory storage adapter
│   ├── bindings.rs         # JavaScript bindings
│   └── types.rs            # WASM-compatible types
├── Cargo.toml              # WASM target configuration
└── pkg/                    # Generated WASM package
    ├── forgedb_wasm.wasm   # Compiled WASM binary
    ├── forgedb_wasm.js     # JS glue code
    └── forgedb_wasm.d.ts   # TypeScript definitions
```

### In-Memory Storage

Since IndexedDB is deferred, use an in-memory storage adapter:

```rust
// Replaces file-based storage with in-memory HashMap
pub struct MemoryStorage {
    data: HashMap<String, Vec<u8>>,
    indexes: HashMap<String, BTreeMap<Vec<u8>, Vec<u64>>>,
}
```

**Benefits**:
- Faster implementation (no IndexedDB async complexity)
- Perfect for: demos, prototyping, temporary data
- Users can persist by exporting/importing JSON

**Limitations**:
- Data lost on page reload
- Limited by browser memory (~2GB)

---

## Implementation Tasks

### Phase 1: WASM Core Setup (3-4 days)

#### Task 1.1: Create WASM Crate
**Estimated**: 2 hours

```toml
# crates/wasm/Cargo.toml
[package]
name = "forgedb-wasm"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
forgedb-core = { path = "../core" }
wasm-bindgen = "0.2"
serde = { version = "1.0", features = ["derive"] }
serde-wasm-bindgen = "0.6"
js-sys = "0.3"
console_error_panic_hook = "0.1"

[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Better optimization
```

**Deliverable**: Cargo.toml configured for WASM target

---

#### Task 1.2: In-Memory Storage Adapter
**Estimated**: 6 hours

Create storage layer that implements the same interface as file-based storage:

```rust
// crates/wasm/src/memory.rs
use std::collections::HashMap;

pub struct MemoryStorage {
    // Key: model_name:record_id, Value: serialized record
    records: HashMap<String, Vec<u8>>,

    // Key: model_name:index_name, Value: index data
    indexes: HashMap<String, BTreeMap<Vec<u8>, Vec<u64>>>,

    // Metadata
    schema: Option<String>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            indexes: HashMap::new(),
            schema: None,
        }
    }

    pub fn get(&self, model: &str, id: &str) -> Option<Vec<u8>> {
        let key = format!("{}:{}", model, id);
        self.records.get(&key).cloned()
    }

    pub fn set(&mut self, model: &str, id: &str, data: Vec<u8>) {
        let key = format!("{}:{}", model, id);
        self.records.insert(key, data);
    }

    pub fn list(&self, model: &str) -> Vec<Vec<u8>> {
        let prefix = format!("{}:", model);
        self.records
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(_, v)| v.clone())
            .collect()
    }

    pub fn delete(&mut self, model: &str, id: &str) -> bool {
        let key = format!("{}:{}", model, id);
        self.records.remove(&key).is_some()
    }
}
```

**Deliverable**: In-memory storage that implements database operations

---

#### Task 1.3: WASM Entry Point
**Estimated**: 4 hours

```rust
// crates/wasm/src/lib.rs
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

mod memory;
use memory::MemoryStorage;

// Set panic hook for better error messages
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct Database {
    storage: MemoryStorage,
    schema: Option<Schema>,
}

#[wasm_bindgen]
impl Database {
    #[wasm_bindgen(constructor)]
    pub fn new(schema_str: &str) -> Result<Database, JsValue> {
        let schema = parse_schema(schema_str)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(Database {
            storage: MemoryStorage::new(),
            schema: Some(schema),
        })
    }

    pub fn get(&self, model: &str, id: &str) -> Result<JsValue, JsValue> {
        let data = self.storage.get(model, id)
            .ok_or_else(|| JsValue::from_str("Record not found"))?;

        // Deserialize and return as JsValue
        let record: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(serde_wasm_bindgen::to_value(&record)?)
    }

    pub fn list(&self, model: &str) -> Result<JsValue, JsValue> {
        let records = self.storage.list(model);

        let parsed: Vec<serde_json::Value> = records
            .iter()
            .filter_map(|data| serde_json::from_slice(data).ok())
            .collect();

        Ok(serde_wasm_bindgen::to_value(&parsed)?)
    }

    // Additional methods: insert, update, delete, query
}
```

**Deliverable**: Basic WASM bindings for database operations

---

### Phase 2: Core Database Features (4-5 days)

#### Task 2.1: Schema Parsing
**Estimated**: 4 hours

Reuse existing schema parser, ensure it works in WASM:

```rust
// crates/wasm/src/schema.rs
use forgedb_core::{parse_schema, Schema};

pub fn parse_and_validate(schema_str: &str) -> Result<Schema, String> {
    parse_schema(schema_str)
        .map_err(|e| format!("Schema parse error: {}", e))
}
```

**Deliverable**: Schema parsing working in WASM

---

#### Task 2.2: CRUD Operations
**Estimated**: 8 hours

Implement all CRUD operations with validation:

```rust
impl Database {
    pub fn insert(&mut self, model: &str, data: JsValue) -> Result<JsValue, JsValue> {
        // Validate against schema
        let schema_model = self.schema.as_ref()
            .and_then(|s| s.get_model(model))
            .ok_or_else(|| JsValue::from_str("Model not found"))?;

        // Convert JsValue to Rust struct
        let record: serde_json::Value = serde_wasm_bindgen::from_value(data)?;

        // Validate types
        validate_record(&record, schema_model)?;

        // Generate ID if needed
        let id = record.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| generate_id());

        // Serialize and store
        let data = serde_json::to_vec(&record)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.storage.set(model, id, data);

        Ok(serde_wasm_bindgen::to_value(&record)?)
    }

    pub fn update(&mut self, model: &str, id: &str, data: JsValue) -> Result<(), JsValue> {
        // Similar implementation
    }

    pub fn delete(&mut self, model: &str, id: &str) -> Result<bool, JsValue> {
        Ok(self.storage.delete(model, id))
    }
}
```

**Deliverable**: Full CRUD operations with validation

---

#### Task 2.3: Query Engine
**Estimated**: 8 hours

Implement query operations (filters, sorting, pagination):

```rust
#[derive(Deserialize)]
pub struct QueryParams {
    filters: Option<serde_json::Value>,
    sort: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl Database {
    pub fn query(&self, model: &str, params: JsValue) -> Result<JsValue, JsValue> {
        let params: QueryParams = serde_wasm_bindgen::from_value(params)?;

        // Get all records
        let mut records = self.storage.list(model);

        // Apply filters
        if let Some(filters) = params.filters {
            records = apply_filters(records, &filters)?;
        }

        // Apply sorting
        if let Some(sort_field) = params.sort {
            records = apply_sorting(records, &sort_field)?;
        }

        // Apply pagination
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(usize::MAX);

        let paginated: Vec<serde_json::Value> = records
            .into_iter()
            .skip(offset)
            .take(limit)
            .filter_map(|data| serde_json::from_slice(&data).ok())
            .collect();

        Ok(serde_wasm_bindgen::to_value(&paginated)?)
    }
}
```

**Deliverable**: Query operations with filters and pagination

---

#### Task 2.4: Indexing
**Estimated**: 6 hours

Implement in-memory indexes for fast lookups:

```rust
impl Database {
    pub fn create_index(&mut self, model: &str, field: &str) -> Result<(), JsValue> {
        let records = self.storage.list(model);

        let mut index = BTreeMap::new();

        for record_data in records {
            let record: serde_json::Value = serde_json::from_slice(&record_data)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;

            if let Some(value) = record.get(field) {
                let key = value.to_string().into_bytes();
                let id = record.get("id").and_then(|v| v.as_u64()).unwrap_or(0);

                index.entry(key)
                    .or_insert_with(Vec::new)
                    .push(id);
            }
        }

        self.storage.add_index(model, field, index);

        Ok(())
    }

    pub fn query_by_index(&self, model: &str, field: &str, value: JsValue)
        -> Result<JsValue, JsValue> {
        // Use index for fast lookup
        let key = serde_wasm_bindgen::from_value::<String>(value)?.into_bytes();

        let ids = self.storage.query_index(model, field, &key)
            .ok_or_else(|| JsValue::from_str("Index not found"))?;

        let records: Vec<serde_json::Value> = ids
            .iter()
            .filter_map(|id| {
                self.storage.get(model, &id.to_string())
                    .and_then(|data| serde_json::from_slice(&data).ok())
            })
            .collect();

        Ok(serde_wasm_bindgen::to_value(&records)?)
    }
}
```

**Deliverable**: Indexing support for fast queries

---

### Phase 3: JavaScript/TypeScript Integration (3-4 days)

#### Task 3.1: Build WASM Package
**Estimated**: 2 hours

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web
wasm-pack build crates/wasm --target web --out-dir ../../wasm/pkg

# Build for Node.js (optional)
wasm-pack build crates/wasm --target nodejs
```

**Configuration**:
```toml
# crates/wasm/Cargo.toml
[package.metadata.wasm-pack.profile.release]
wasm-opt = ["-Oz", "--enable-mutable-globals"]
```

**Deliverable**: WASM package built and ready for JavaScript

---

#### Task 3.2: TypeScript Wrapper
**Estimated**: 6 hours

Create ergonomic TypeScript API:

```typescript
// wasm/src/Database.ts
import init, { Database as WasmDatabase } from '../pkg/forgedb_wasm';

export class Database<T extends Record<string, any> = any> {
  private db: WasmDatabase | null = null;
  private initialized = false;

  async init(schema: string): Promise<void> {
    if (!this.initialized) {
      await init(); // Initialize WASM module
      this.initialized = true;
    }
    this.db = new WasmDatabase(schema);
  }

  async get<M extends keyof T>(
    model: M,
    id: string
  ): Promise<T[M] | null> {
    if (!this.db) throw new Error('Database not initialized');

    try {
      const result = this.db.get(model as string, id);
      return result as T[M];
    } catch (e) {
      if (e instanceof Error && e.message.includes('not found')) {
        return null;
      }
      throw e;
    }
  }

  async list<M extends keyof T>(
    model: M,
    options?: {
      filters?: Partial<T[M]>;
      sort?: keyof T[M];
      limit?: number;
      offset?: number;
    }
  ): Promise<T[M][]> {
    if (!this.db) throw new Error('Database not initialized');

    const result = this.db.query(model as string, options || {});
    return result as T[M][];
  }

  async insert<M extends keyof T>(
    model: M,
    data: Omit<T[M], 'id'> & { id?: string }
  ): Promise<T[M]> {
    if (!this.db) throw new Error('Database not initialized');

    const result = this.db.insert(model as string, data);
    return result as T[M];
  }

  async update<M extends keyof T>(
    model: M,
    id: string,
    data: Partial<T[M]>
  ): Promise<void> {
    if (!this.db) throw new Error('Database not initialized');

    this.db.update(model as string, id, data);
  }

  async delete<M extends keyof T>(
    model: M,
    id: string
  ): Promise<boolean> {
    if (!this.db) throw new Error('Database not initialized');

    return this.db.delete(model as string, id);
  }

  // Export/import for persistence
  export(): string {
    if (!this.db) throw new Error('Database not initialized');
    return this.db.export_json();
  }

  import(json: string): void {
    if (!this.db) throw new Error('Database not initialized');
    this.db.import_json(json);
  }
}
```

**Deliverable**: Type-safe TypeScript API

---

#### Task 3.3: Type Generation from Schema
**Estimated**: 6 hours

Generate TypeScript types from schema:

```typescript
// wasm/src/codegen.ts
export function generateTypes(schema: string): string {
  // Parse schema and generate TypeScript interfaces
  const models = parseSchema(schema);

  let output = '// Auto-generated from schema\n\n';

  for (const model of models) {
    output += `export interface ${model.name} {\n`;

    for (const field of model.fields) {
      const optional = field.required ? '' : '?';
      output += `  ${field.name}${optional}: ${mapType(field.type)};\n`;
    }

    output += '}\n\n';
  }

  // Generate schema type
  output += 'export interface Schema {\n';
  for (const model of models) {
    output += `  ${model.name}: ${model.name};\n`;
  }
  output += '}\n';

  return output;
}

function mapType(forgeType: string): string {
  const typeMap: Record<string, string> = {
    'string': 'string',
    'i32': 'number',
    'i64': 'number',
    'f64': 'number',
    'bool': 'boolean',
    'uuid': 'string',
    'timestamp': 'string',
    'json': 'any',
  };

  return typeMap[forgeType] || 'any';
}
```

**Deliverable**: TypeScript type generation

---

#### Task 3.4: Example Applications
**Estimated**: 4 hours

Create demo applications:

```html
<!-- wasm/examples/todo-app.html -->
<!DOCTYPE html>
<html>
<head>
  <title>ForgeDB WASM Todo App</title>
  <script type="module">
    import { Database } from '../pkg/forgedb_wasm.js';

    const schema = `
      Todo {
        id: +uuid
        title: string
        completed: bool
        created_at: timestamp
      }
    `;

    async function main() {
      const db = new Database();
      await db.init(schema);

      // Add todo
      const todo = await db.insert('Todo', {
        title: 'Learn ForgeDB WASM',
        completed: false,
        created_at: new Date().toISOString(),
      });

      console.log('Created todo:', todo);

      // List todos
      const todos = await db.list('Todo');
      console.log('All todos:', todos);

      // Export data
      const exported = db.export();
      localStorage.setItem('todos', exported);
    }

    main();
  </script>
</head>
<body>
  <h1>ForgeDB WASM Todo App</h1>
  <div id="app"></div>
</body>
</html>
```

**Deliverable**: Working demo applications

---

### Phase 4: Testing & Optimization (2-3 days)

#### Task 4.1: Unit Tests
**Estimated**: 6 hours

```rust
// crates/wasm/tests/web.rs
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use forgedb_wasm::Database;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_create_database() {
    let schema = "User { id: +uuid, name: string }";
    let db = Database::new(schema);
    assert!(db.is_ok());
}

#[wasm_bindgen_test]
fn test_insert_and_get() {
    let schema = "User { id: +uuid, name: string, email: string }";
    let mut db = Database::new(schema).unwrap();

    // Insert
    let data = serde_json::json!({
        "id": "1",
        "name": "Alice",
        "email": "alice@example.com"
    });

    db.insert("User", serde_wasm_bindgen::to_value(&data).unwrap()).unwrap();

    // Get
    let result = db.get("User", "1").unwrap();
    assert!(result.is_truthy());
}

#[wasm_bindgen_test]
fn test_query_with_filters() {
    let schema = "User { id: +uuid, name: string, age: i32 }";
    let mut db = Database::new(schema).unwrap();

    // Insert test data
    for i in 1..=10 {
        let data = serde_json::json!({
            "id": i.to_string(),
            "name": format!("User{}", i),
            "age": 20 + i
        });
        db.insert("User", serde_wasm_bindgen::to_value(&data).unwrap()).unwrap();
    }

    // Query with filter
    let params = serde_json::json!({
        "filters": { "age": 25 },
        "limit": 5
    });

    let results = db.query("User", serde_wasm_bindgen::to_value(&params).unwrap()).unwrap();
    assert!(results.is_truthy());
}
```

**Run tests**:
```bash
wasm-pack test --headless --firefox crates/wasm
```

**Deliverable**: Comprehensive test suite

---

#### Task 4.2: Size Optimization
**Estimated**: 4 hours

Reduce WASM binary size:

```toml
# Cargo.toml optimizations
[profile.release]
opt-level = "z"           # Optimize for size
lto = true                # Link-time optimization
codegen-units = 1         # Better optimization
strip = true              # Strip debug symbols
panic = "abort"           # Smaller panic handling

[dependencies]
# Use minimal dependencies
wee_alloc = "0.4"         # Tiny allocator
```

```rust
// Use wee_alloc
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
```

**Target**: <500KB gzipped WASM binary

**Deliverable**: Optimized WASM binary

---

#### Task 4.3: Performance Benchmarks
**Estimated**: 3 hours

```typescript
// wasm/benchmarks/perf.ts
import { Database } from '../pkg';

async function benchmark() {
  const db = new Database();
  await db.init('User { id: +uuid, name: string, age: i32 }');

  console.time('Insert 1000 records');
  for (let i = 0; i < 1000; i++) {
    await db.insert('User', {
      name: `User${i}`,
      age: 20 + (i % 50),
    });
  }
  console.timeEnd('Insert 1000 records');

  console.time('Query with filter');
  const results = await db.list('User', {
    filters: { age: 25 },
  });
  console.timeEnd('Query with filter');

  console.log(`Found ${results.length} results`);
}

benchmark();
```

**Target Performance**:
- Insert: <1ms per record
- Query: <10ms for 1000 records
- List: <5ms for 100 records

**Deliverable**: Performance benchmarks

---

## Package Distribution

### NPM Package Structure

```
@forgedb/wasm/
├── package.json
├── README.md
├── pkg/
│   ├── forgedb_wasm.wasm
│   ├── forgedb_wasm.js
│   └── forgedb_wasm.d.ts
└── src/
    ├── Database.ts
    └── index.ts
```

### package.json

```json
{
  "name": "@forgedb/wasm",
  "version": "0.1.0",
  "description": "ForgeDB WebAssembly bindings for browser",
  "type": "module",
  "main": "./src/index.js",
  "types": "./src/index.d.ts",
  "files": [
    "pkg",
    "src"
  ],
  "exports": {
    ".": {
      "types": "./src/index.d.ts",
      "import": "./src/index.js"
    }
  },
  "keywords": [
    "database",
    "wasm",
    "webassembly",
    "forgedb",
    "in-memory"
  ],
  "repository": {
    "type": "git",
    "url": "https://github.com/forgedb/forgedb"
  }
}
```

---

## Testing Strategy

### Browser Testing
```bash
# Firefox
wasm-pack test --headless --firefox

# Chrome
wasm-pack test --headless --chrome

# Safari (macOS only)
wasm-pack test --headless --safari
```

### Integration Testing
```typescript
// Test in real browser environment
import { test } from '@playwright/test';
import { Database } from '@forgedb/wasm';

test('CRUD operations', async ({ page }) => {
  await page.goto('http://localhost:3000');

  const result = await page.evaluate(async () => {
    const db = new Database();
    await db.init('User { id: +uuid, name: string }');

    const user = await db.insert('User', { name: 'Alice' });
    const found = await db.get('User', user.id);

    return found.name === 'Alice';
  });

  expect(result).toBe(true);
});
```

---

## Documentation

### README.md

```markdown
# ForgeDB WASM

WebAssembly bindings for ForgeDB - a schema-first database.

## Features

- 🚀 Zero setup - runs entirely in the browser
- 💾 In-memory storage (export/import for persistence)
- 🔍 Full query engine with filters and indexes
- 📝 Type-safe TypeScript API
- 🎯 Small bundle size (<500KB gzipped)

## Installation

\`\`\`bash
npm install @forgedb/wasm
\`\`\`

## Quick Start

\`\`\`typescript
import { Database } from '@forgedb/wasm';

// Define schema
const schema = \`
  User {
    id: +uuid
    name: string
    email: string
    age: i32
  }
\`;

// Initialize database
const db = new Database();
await db.init(schema);

// Insert records
const user = await db.insert('User', {
  name: 'Alice',
  email: 'alice@example.com',
  age: 30,
});

// Query records
const users = await db.list('User', {
  filters: { age: 30 },
  limit: 10,
});

// Export for persistence
const data = db.export();
localStorage.setItem('db', data);

// Import on next load
db.import(localStorage.getItem('db'));
\`\`\`

## Limitations

- In-memory only (data lost on reload without export/import)
- No server-side features (API generation, SSR)
- ~2GB memory limit (browser dependent)

## Future

- IndexedDB persistence
- Service Worker integration
- Sync with server
\`\`\`

---

## Success Criteria

### Functional Requirements
- ✅ All CRUD operations working
- ✅ Query engine with filters
- ✅ Schema validation
- ✅ Index support
- ✅ Export/import JSON
- ✅ Type-safe TypeScript API

### Non-Functional Requirements
- ✅ WASM binary <500KB gzipped
- ✅ Insert <1ms per record
- ✅ Query <10ms for 1000 records
- ✅ All tests passing in 3+ browsers
- ✅ Documentation complete

### Deliverables
- ✅ NPM package published
- ✅ Example applications
- ✅ Benchmarks documented
- ✅ README with usage guide

---

## Future Enhancements (Out of Scope for Initial Release)

### IndexedDB Persistence
- Async storage adapter
- Transaction support
- Quota management

### Service Worker Integration
- Offline-first support
- Background sync
- Cache strategies

### Server Sync
- Conflict resolution
- Incremental sync
- Real-time updates

---

## Timeline

| Phase | Tasks | Duration |
|-------|-------|----------|
| **Phase 1: WASM Core Setup** | 1.1-1.3 | 3-4 days |
| **Phase 2: Database Features** | 2.1-2.4 | 4-5 days |
| **Phase 3: JS/TS Integration** | 3.1-3.4 | 3-4 days |
| **Phase 4: Testing & Optimization** | 4.1-4.3 | 2-3 days |
| **Total** | | **12-16 days** |

**Buffer**: +2-3 days for unexpected issues

**Total Estimated**: **2-3 weeks**

---

## Risk Mitigation

### Risk 1: WASM Bundle Size
**Mitigation**: Aggressive optimization, feature flags, lazy loading

### Risk 2: Browser Compatibility
**Mitigation**: Polyfills, graceful degradation, browser testing

### Risk 3: Performance in Browser
**Mitigation**: Early benchmarks, profiling, optimization

### Risk 4: TypeScript Integration Complexity
**Mitigation**: Use proven patterns (wasm-bindgen, serde-wasm-bindgen)

---

**Status**: 📋 Planning
**Last Updated**: 2025-10-15
**Next Step**: Create WASM crate and begin Phase 1
