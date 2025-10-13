# SinkDB Advanced Features Addendum

## Version 0.2.0 - Extended Feature Set

This document captures advanced features and ideas beyond the initial v1.0 specification.

---

## Feature 1: Partial Column Selection (Projection)

### Overview

Only load columns actually needed for a query, reducing memory bandwidth and deserialization overhead.

### Schema Annotation

```
User {
  id: +uuid
  email: ^&string
  password_hash: #argon2(32) @private
  bio: string
  avatar_url: string?
  
  // Rarely accessed together
  settings: Settings @cold
  analytics: Analytics @cold
}
```

### Storage Implications

Columns are already separate in storage, so this is primarily about:
1. Not loading unnecessary columns into cache
2. Skipping deserialization for unused fields
3. API-level field selection

### Query API

```rust
// Load only specific fields
db.users()
  .select(&["id", "email", "bio"])
  .filter(|u| u.age > 25)
  .collect()

// Generated code only touches selected columns
fn query_users_partial(fields: &[&str]) -> Vec<UserPartial> {
    // Only mmap/load requested columns
    let ids = if fields.contains("id") { 
        Some(self.load_column("User.id")) 
    } else { 
        None 
    };
    let emails = if fields.contains("email") { 
        Some(self.load_column("User.email")) 
    } else { 
        None 
    };
    // ...
}
```

### HTTP API

```bash
# Only return id and email
GET /api/users?fields=id,email

# Exclude heavy fields
GET /api/users?exclude=bio,settings,analytics
```

### Performance Benefits

- **Memory**: 10x reduction when selecting 1 of 10 columns
- **Cache**: More rows fit in L1/L2/L3
- **Network**: Smaller JSON payloads
- **Deserialization**: Skip unused fields entirely

---

## Feature 2: Hot Cache / Materialized In-Memory Views

### Overview

Keep frequently accessed data in memory in a row-oriented format for ultra-fast access, while keeping columnar storage for analytics.

### Schema Annotation

```
User {
  @hot_cache(size: 10000, strategy: "lru")
  
  id: +uuid
  email: ^&string
  name: string
  // ...
}

Product {
  @hot_cache(size: 1000, strategy: "lru")
  @hot_fields(["id", "name", "price", "inventory"])  // Only cache these
  
  id: +uuid
  name: string
  price: $USD
  inventory: i32
  // Heavy fields not cached
  description: string
  reviews: [Review]
}
```

### Implementation

```rust
struct HotCache {
    // Row-oriented cache for fast single-record access
    cache: LruCache<uuid, CachedUser>,
    
    // Metadata
    hits: AtomicU64,
    misses: AtomicU64,
}

struct CachedUser {
    // Materialized row
    id: uuid,
    email: String,
    name: String,
    age: u32,
    // All frequently accessed fields
}

impl DB {
    fn get_user_hot(&self, id: uuid) -> Option<&CachedUser> {
        if let Some(user) = self.hot_cache.get(&id) {
            self.hot_cache.hits.fetch_add(1);
            return Some(user);
        }
        
        self.hot_cache.misses.fetch_add(1);
        
        // Cache miss: load from columnar storage
        let user = self.load_user_from_columns(id)?;
        let cached = CachedUser::from(user);
        self.hot_cache.insert(id, cached);
        
        Some(self.hot_cache.get(&id).unwrap())
    }
}
```

### Cache Strategies

```toml
[cache]
strategy = "lru"              # Least Recently Used
size = 10000                  # Number of records
memory_limit = "256MB"        # Or memory-based limit
ttl = 300                     # Time-to-live in seconds
preload = ["top_users"]       # Preload specific queries

[cache.invalidation]
on_write = true               # Invalidate on update
write_through = true          # Update cache on write
```

### Hybrid Access Pattern

```rust
// Fast path: hot cache (row-oriented)
let user = db.get_user_hot(id)?;  // < 100ns (L1 cache)

// Analytical path: columnar scan
let users = db.users()
    .filter_age_gt(25)           // Columnar vectorized scan
    .collect();                  // Materialize results

// Best of both worlds
let popular_products = db.products()
    .hot_cache()                 // Use cache for these
    .filter(|p| p.inventory > 0)
    .limit(100)
    .collect();
```

### Benefits

- **Point queries**: 10-100x faster (nanoseconds vs microseconds)
- **Row reconstruction**: No column reconstruction needed
- **Popular data**: Keep hot data always in memory
- **Hybrid workload**: OLTP + OLAP in same database

---

## Feature 3: Tuple Types

### Overview

Store multiple related records in a single field without creating a junction table.

### Syntax

```
Child {
  id: +uuid
  name: string
  
  // Tuple: exactly 2 parents
  parents: (Parent, Parent)
  
  // Named tuple fields
  parents: (mother: Parent, father: Parent)
  
  // Optional tuple
  guardians: (Parent, Parent)?
}

Meeting {
  id: +uuid
  title: string
  
  // Triple
  participants: (User, User, User)
  
  // Variable-length is still array
  attendees: [User]
}

Coordinate {
  id: +uuid
  
  // Tuples can be inline types too
  point_2d: (f64, f64)
  point_3d: (f64, f64, f64)
  
  // Or named
  bounds: (min: f64, max: f64)
}
```

### Storage

**Fixed-size tuples** (like inline structs):
```
parents column = [
  (mom_id_1, dad_id_1),
  (mom_id_2, dad_id_2),
  ...
]
```

Each tuple element is a foreign key (uuid), so:
- `(Parent, Parent)` = 32 bytes per child (16 + 16)
- Memory-mapped, zero-copy access
- Sequential layout

**Named tuple access:**
```rust
let child = db.get_child(id)?;
let mother = db.get_parent(child.parents.mother)?;
let father = db.get_parent(child.parents.father)?;
```

### TypeScript Generation

```typescript
type Child = {
  id: string
  name: string
  parents: {
    mother: Parent
    father: Parent
  }
}

// Or unnamed
type Meeting = {
  participants: [User, User, User]
}
```

### API

```bash
# Create with tuple
POST /api/children
{
  "name": "Alice",
  "parents": {
    "mother": "uuid-1",
    "father": "uuid-2"
  }
}

# Include tuple relations
GET /api/children/123?include=parents.mother,parents.father
```

### Use Cases

- **Dual relationships**: mother/father, buyer/seller, sender/receiver
- **Fixed participants**: 1v1 game (player1, player2)
- **Coordinate pairs**: (lat, lon), (min, max)
- **Multi-part keys**: (year, month, day)

### Constraints

```
Child {
  parents: (mother: Parent, father: Parent) {
    unique_together: true  // Can't have same parent twice
    validate: different_parents
  }
}
```

---

## Feature 4: Blockchain-Based Distributed Transaction Ledger

### Concept

Use blockchain consensus for distributed transactions across nodes in a SinkDB cluster.

### Architecture

```
Node A (SF)          Node B (NYC)         Node C (LON)
    |                    |                    |
    |                    |                    |
    +-------- Blockchain Transaction Ledger --------+
              (Append-only, cryptographically linked)
```

### Transaction Flow

```
1. Client → Write Request → Node A
2. Node A → Propose Transaction → Blockchain
3. Consensus Algorithm (Raft/PBFT) → Validate
4. Blockchain → Commit Block (tx_id, data_hash, prev_hash, signature)
5. All Nodes → Apply Transaction → Local Storage
6. Node A → Acknowledge → Client
```

### Blockchain Structure

```rust
struct Block {
    height: u64,
    timestamp: u64,
    prev_hash: [u8; 32],
    transactions: Vec<Transaction>,
    merkle_root: [u8; 32],
    signatures: Vec<NodeSignature>,  // Multi-sig from quorum
}

struct Transaction {
    tx_id: uuid,
    model: ModelId,
    operation: Operation,  // Insert/Update/Delete
    data: Vec<u8>,
    timestamp: u64,
    node_id: NodeId,
    signature: [u8; 64],
}

enum Operation {
    Insert { model: u16, data: Vec<u8> },
    Update { model: u16, id: uuid, fields: Vec<(u16, Vec<u8>)> },
    Delete { model: u16, id: uuid },
}
```

### Consensus

**Raft-inspired for simplicity:**
```
1. Leader election
2. Leader proposes block
3. Followers validate and sign
4. Quorum (N/2 + 1) → commit
5. Leader broadcasts committed block
```

**Or PBFT for Byzantine fault tolerance:**
- Handles malicious nodes
- 3f + 1 nodes to tolerate f failures

### Benefits

1. **Immutable audit trail**: Every write is cryptographically linked
2. **Byzantine fault tolerance**: Survives malicious nodes (with PBFT)
3. **Distributed consensus**: No single point of failure
4. **Cryptographic verification**: Each transaction signed
5. **Time-ordered**: Blockchain provides global order
6. **Replay**: Can reconstruct state from genesis block

### Schema-Driven Validation

```
User {
  @replicated
  @audit_blockchain
  
  id: +uuid
  email: string
  
  // Blockchain enforces these
  @validate_on_consensus
}
```

Nodes validate schema constraints during consensus:
- Unique constraint violations rejected
- Foreign key integrity checked
- Type validation enforced

### Performance Considerations

- **Block time**: 1-5 seconds (configurable)
- **Batch transactions**: Multiple txs per block
- **Async replication**: Leader responds before full commit (with risk)
- **Partitioning**: Shard data across multiple chains

### Configuration

```toml
[distributed]
mode = "blockchain"
nodes = ["node1:5000", "node2:5000", "node3:5000"]
consensus = "raft"  # or "pbft"

[blockchain]
block_time = 2000            # ms
max_transactions_per_block = 1000
signature_algorithm = "ed25519"
hash_algorithm = "sha256"

[consensus]
quorum_size = 2              # N/2 + 1 for 3 nodes
election_timeout = 5000      # ms
heartbeat_interval = 1000    # ms
```

---

## Feature 5: WASM Browser Sync with Live Updates

### Overview

Browser WASM instance acts as a **first-class distributed node** with push-based live sync from server.

### Architecture

```
Server Node (Leader)
    |
    | (WebSocket / Server-Sent Events)
    |
Browser WASM Node (Follower)
    |
    +-- IndexedDB (persistent storage)
    +-- In-memory cache
    +-- Query engine (columnar, same as server)
```

### Distributed Membership

Browser is a **read-only replica** in the distributed system:

```rust
enum NodeType {
    Leader,      // Can write, proposes transactions
    Follower,    // Reads from blockchain, applies txs
    Browser,     // Special follower: WASM, limited resources
}
```

### Live Sync Protocol

**1. Initial sync:**
```
Browser → Server: SYNC_REQUEST { models: ["User", "Post"], last_block: 0 }
Server → Browser: SNAPSHOT { block_height: 1234, data: [...] }
Browser → IndexedDB: Store snapshot
```

**2. Subscribe to changes:**
```
Browser → Server: SUBSCRIBE { models: ["Post"], filters: { status: "published" } }
Server → Browser: SUBSCRIBED { subscription_id: "abc" }
```

**3. Receive live updates:**
```
Server → Browser: UPDATE {
  subscription_id: "abc",
  block_height: 1235,
  transaction: {
    operation: "INSERT",
    model: "Post",
    data: { id: "...", title: "New Post", ... }
  }
}

Browser: Apply transaction to local storage
Browser: Emit event for UI reactivity
```

### Schema-Driven Subscriptions

```
// In schema
Post {
  @live_sync(strategy: "incremental")
  
  id: +uuid
  title: string
  status: string
  
  // Only sync published posts to browsers
  @sync_filter(status: "published")
}

User {
  @live_sync(strategy: "full_snapshot")
  @sync_fields(["id", "email", "name"])  // Exclude sensitive fields
  
  id: +uuid
  email: string
  password_hash: #argon2(32) @private @no_sync
}
```

### Push Mechanisms

**WebSocket (bidirectional):**
```javascript
const ws = new WebSocket('wss://api.example.com/sync')

ws.onopen = () => {
  ws.send(JSON.stringify({
    type: 'SUBSCRIBE',
    models: ['Post', 'Comment'],
    filters: { 'Post.status': 'published' }
  }))
}

ws.onmessage = (event) => {
  const update = JSON.parse(event.data)
  // { type: 'UPDATE', model: 'Post', operation: 'INSERT', data: {...} }
  
  await db.applyTransaction(update)
  
  // Trigger reactivity
  postStore.invalidate()
}
```

**Server-Sent Events (unidirectional, simpler):**
```javascript
const events = new EventSource('/api/sync/stream')

events.addEventListener('post:insert', (e) => {
  const post = JSON.parse(e.data)
  await db.posts.insert(post)
})
```

### Conflict Resolution

Browser is **read-only** by default, but can have local writes:

**Optimistic UI + Server reconciliation:**
```javascript
// Local write (optimistic)
const post = await db.posts.create({
  title: 'Draft',
  status: 'draft'
})  // Immediately reflected in UI

// Background sync to server
await syncToServer({
  operation: 'INSERT',
  model: 'Post',
  data: post,
  temp_id: post.id
})

// Server responds with canonical ID
// { temp_id: "local-123", server_id: "uuid-456" }

// Update local record
await db.posts.update(post.id, { id: server_id })
```

**Conflict resolution strategies:**
```toml
[sync]
strategy = "server_wins"  # or "last_write_wins", "custom"

[sync.conflict]
on_conflict = "merge"     # or "abort", "manual"
merge_strategy = "deep"   # Field-level merge
```

### Offline Support

```javascript
// Browser WASM instance works offline
const db = await SinkDB.init({
  online: false,  // Start offline
  syncWhenOnline: true
})

// Queue writes while offline
await db.posts.create({ title: "Offline Post" })
// Stored locally, queued for sync

// Reconnect
await db.goOnline()
// Syncs queued writes, pulls server updates
```

### Incremental Sync Strategies

**Event-based (fine-grained):**
```
Server: INSERT Post(id=1)
→ Browser: Apply insert

Server: UPDATE Post(id=1, title="New Title")
→ Browser: Apply update
```

**Snapshot + delta (coarse-grained):**
```
Browser: Request snapshot at block 1000
Server: Send snapshot

Server: Blocks 1001-1234 available
Browser: Request delta
Server: Send only changes since block 1000
```

**Merkle tree verification:**
```
Browser: Send Merkle root of local state
Server: Compare roots
Server: Send diff (only changed subtrees)
Browser: Apply diff
```

### Schema

```
Post {
  @live_sync
  @sync_strategy("incremental")
  
  id: +uuid
  title: string
  
  // Sync metadata (auto-generated)
  _sync_version: u64 @internal
  _sync_hash: #sha256(32) @internal
}
```

### Performance

- **Binary protocol**: Use MessagePack or custom binary
- **Compression**: Gzip deltas
- **Batching**: Group updates into batches
- **Debouncing**: Don't flood on rapid changes

### React Integration

```jsx
import { useLiveQuery } from './generated/hooks'

function PostList() {
  // Auto-updates when server pushes changes
  const posts = useLiveQuery(
    db.posts
      .where({ status: 'published' })
      .sort('-created_at')
  )
  
  return (
    <div>
      {posts.map(post => (
        <PostCard key={post.id} post={post} />
      ))}
    </div>
  )
}
```

### Benefits

1. **Real-time UIs**: No polling, instant updates
2. **Offline-first**: Full query engine in browser
3. **Reduced latency**: Local queries, no server round-trip
4. **Scalability**: Offload reads to browsers
5. **Consistency**: Blockchain ensures global order
6. **Type safety**: Same schema, same types, everywhere

---

## Integration: All Features Together

### Example Schema

```
User {
  @hot_cache(size: 10000)
  @replicated
  @live_sync
  
  id: +uuid
  email: ^&string
  name: string
  
  // Heavy fields not in hot cache
  bio: string @cold
  settings: Settings @cold
  
  password_hash: #argon2(32) @private @no_sync
  
  parents: (mother: User, father: User)?
  
  created_at: +timestamp
}

Post {
  @live_sync(strategy: "incremental")
  @sync_filter(status: "published")
  
  id: +uuid
  title: string
  content: string
  status: string
  
  // Tuple for co-authors
  authors: (primary: User, secondary: User?)
  
  view_count: +u64 @hot_cache
  
  created_at: +timestamp
}
```

### Query

```rust
// Fast path: hot cache + partial projection
let user = db.users()
    .hot()                           // Use hot cache
    .select(&["id", "email", "name"]) // Partial columns
    .get(user_id)?;

// Analytical query: columnar scan
let popular_posts = db.posts()
    .filter_vectorized(|p| p.view_count > 1000)
    .select(&["id", "title", "view_count"])
    .limit(100)
    .collect();

// Tuple access
let post = db.posts().get(post_id)?;
let primary_author = db.users().get(post.authors.primary)?;
```

### Browser

```javascript
// Initialize WASM with live sync
const db = await SinkDB.init({
  url: 'wss://api.example.com',
  liveSync: true,
  models: ['Post', 'User'],
  filters: {
    'Post.status': 'published'
  }
})

// Query locally (instant, no network)
const posts = await db.posts
  .where({ status: 'published' })
  .orderBy('-created_at')
  .limit(10)
  .toArray()

// Automatically updates when server pushes
db.posts.subscribe((change) => {
  console.log('Post changed:', change)
  // { type: 'insert', record: {...} }
})
```

---

## Implementation Roadmap

### Phase 2.5: Hot Cache + Partial Selection
- **Timeline**: 2-3 months after v2.0
- In-memory LRU cache for hot records
- Query optimizer for projection
- Field selection in API

### Phase 3.1: Tuple Types
- **Timeline**: Part of v3.0
- Parser support for tuple syntax
- Storage layout for fixed-size tuples
- TypeScript type generation

### Phase 3.2: Blockchain Consensus (Experimental)
- **Timeline**: 6-12 months after v3.0
- Raft consensus implementation
- Blockchain block structure
- Multi-node testing

### Phase 3.3: WASM Live Sync
- **Timeline**: Part of v3.0
- WASM compilation target
- WebSocket/SSE sync protocol
- IndexedDB persistence
- Conflict resolution
- React hooks

### Phase 3.4: Custom Macro System
- **Timeline**: Part of v3.0 or standalone release
- Macro parser (comment-based syntax)
- Compile-time macro execution engine
- Runtime decorator generation
- Macro registry and plugin system
- Security sandboxing
- CLI integration for macro testing

---

## Feature 6: Custom Macro System

### Overview

TypeScript-based macro system for compile-time code generation and runtime metaprogramming. Macros are defined in comments and implemented in TypeScript, providing powerful extensibility without introducing a new language.

### Macro Types

**1. Compile-Time Macros**
- Execute during `sinkdb generate`
- Can modify generated code
- Access to AST and schema
- Useful for: code generation, embedding metadata, optimization

**2. Runtime Macros**
- Generate decorators/wrappers
- Execute during application runtime
- Useful for: validation, logging, caching, auth

### Syntax

#### In Schema

```
User {
  /** 
   * @macro compile_time generate_id
   * @macro_config { version: 7, node_id: 1 }
   */
  id: uuid
  
  /** 
   * @macro runtime validate
   * @macro runtime sanitize
   * @macro_impl ./macros/email-validator.ts
   */
  email: string
  
  /**
   * @macro compile_time embed_build_info
   */
  _build: BuildInfo @internal
}

struct BuildInfo {
  /**
   * @macro compile_time git_hash
   */
  git_hash: char(40)
  
  /**
   * @macro compile_time build_timestamp
   */
  build_time: timestamp
  
  /**
   * @macro compile_time semver
   * @macro_source ./package.json:version
   */
  version: char(20)
}
```

#### In TypeScript

```typescript
/// @macro compile_time precompute_constants
const TAX_RATES = {
  /** @macro compile_time calculate_rate(state: "CA", year: 2025) */
  CA: 0.0725,
  
  /** @macro compile_time calculate_rate(state: "NY", year: 2025) */
  NY: 0.08875
}

/// @macro runtime memoize(ttl: 3600)
/// @macro runtime log_performance
export function expensiveCalculation(input: number): number {
  // Complex calculation
  return input * 42
}

/// @macro runtime require_auth
/// @macro runtime rate_limit(rpm: 100)
export async function createUser(data: UserCreate) {
  // ...
}
```

### Compile-Time Macro Implementation

#### Macro Definition

```typescript
// macros/git-info.ts

import { CompileTimeMacro, MacroContext } from 'sinkdb/macros'
import { execSync } from 'child_process'

export const git_hash: CompileTimeMacro = {
  name: 'git_hash',
  
  // Execute during code generation
  execute(ctx: MacroContext): string {
    const hash = execSync('git rev-parse HEAD')
      .toString()
      .trim()
    
    console.log(`[macro] Embedding git hash: ${hash}`)
    
    return hash
  }
}

export const build_timestamp: CompileTimeMacro = {
  name: 'build_timestamp',
  
  execute(ctx: MacroContext): number {
    return Date.now()
  }
}

export const semver: CompileTimeMacro = {
  name: 'semver',
  
  execute(ctx: MacroContext): string {
    const { source } = ctx.config // ./package.json:version
    const [file, path] = source.split(':')
    
    const pkg = require(`./${file}`)
    const version = path.split('.').reduce((obj, key) => obj[key], pkg)
    
    return version
  }
}
```

#### Generated Code

```typescript
// generated/types.ts

export type BuildInfo = {
  git_hash: string    // "a3f5c8d9e2b1f4c7a6d8e9f0a1b2c3d4e5f6a7b8"
  build_time: number  // 1697123456789
  version: string     // "1.2.3"
}

// generated/constants.ts
export const BUILD_INFO: BuildInfo = {
  git_hash: "a3f5c8d9e2b1f4c7a6d8e9f0a1b2c3d4e5f6a7b8",
  build_time: 1697123456789,
  version: "1.2.3"
}
```

### Runtime Macro Implementation

#### Macro Definition

```typescript
// macros/validation.ts

import { RuntimeMacro, MacroContext } from 'sinkdb/macros'

export const validate: RuntimeMacro = {
  name: 'validate',
  
  // Returns decorator factory
  generate(ctx: MacroContext): PropertyDecorator {
    const { field, schema } = ctx
    
    return function(target: any, propertyKey: string | symbol) {
      // Get validation rules from schema
      const rules = schema.fields[propertyKey]?.validation || {}
      
      // Create getter/setter with validation
      let value: any
      
      Object.defineProperty(target, propertyKey, {
        get() { return value },
        set(newValue: any) {
          // Apply validation
          if (rules.pattern && !new RegExp(rules.pattern).test(newValue)) {
            throw new Error(`${propertyKey} does not match pattern`)
          }
          
          if (rules.min_length && newValue.length < rules.min_length) {
            throw new Error(`${propertyKey} too short`)
          }
          
          value = newValue
        }
      })
    }
  }
}

export const sanitize: RuntimeMacro = {
  name: 'sanitize',
  
  generate(ctx: MacroContext): PropertyDecorator {
    return function(target: any, propertyKey: string | symbol) {
      let value: any
      
      Object.defineProperty(target, propertyKey, {
        get() { return value },
        set(newValue: any) {
          // Sanitize input
          value = typeof newValue === 'string' 
            ? newValue.trim().toLowerCase()
            : newValue
        }
      })
    }
  }
}
```

#### Generated Code with Decorators

```typescript
// generated/models.ts

import { validate, sanitize } from '../macros/validation'

export class User {
  id: string
  
  @validate
  @sanitize
  email: string
  
  constructor(data: Partial<User>) {
    Object.assign(this, data)
  }
}

// Usage
const user = new User({ email: "  ALICE@EXAMPLE.COM  " })
console.log(user.email) // "alice@example.com" (sanitized)

user.email = "invalid"  // Throws: does not match pattern
```

### Function Macros

```typescript
// macros/performance.ts

import { RuntimeMacro } from 'sinkdb/macros'

export const memoize: RuntimeMacro = {
  name: 'memoize',
  
  generate(ctx: MacroContext): MethodDecorator {
    const { ttl = 3600 } = ctx.config
    const cache = new Map<string, { value: any, expires: number }>()
    
    return function(
      target: any,
      propertyKey: string | symbol,
      descriptor: PropertyDescriptor
    ) {
      const original = descriptor.value
      
      descriptor.value = function(...args: any[]) {
        const key = JSON.stringify(args)
        const cached = cache.get(key)
        
        if (cached && Date.now() < cached.expires) {
          return cached.value
        }
        
        const result = original.apply(this, args)
        cache.set(key, {
          value: result,
          expires: Date.now() + (ttl * 1000)
        })
        
        return result
      }
    }
  }
}

export const log_performance: RuntimeMacro = {
  name: 'log_performance',
  
  generate(ctx: MacroContext): MethodDecorator {
    return function(
      target: any,
      propertyKey: string | symbol,
      descriptor: PropertyDescriptor
    ) {
      const original = descriptor.value
      
      descriptor.value = async function(...args: any[]) {
        const start = performance.now()
        const result = await original.apply(this, args)
        const duration = performance.now() - start
        
        console.log(`[perf] ${String(propertyKey)}: ${duration.toFixed(2)}ms`)
        
        return result
      }
    }
  }
}
```

### Advanced Compile-Time Macros

#### Code Generation

```typescript
// macros/crud-generator.ts

import { CompileTimeMacro, MacroContext } from 'sinkdb/macros'

export const generate_crud: CompileTimeMacro = {
  name: 'generate_crud',
  
  execute(ctx: MacroContext): string {
    const { model, schema } = ctx
    
    return `
      export class ${model}Service {
        async create(data: ${model}Create): Promise<${model}> {
          // Generated CRUD
          return db.${model.toLowerCase()}s.create(data)
        }
        
        async findById(id: string): Promise<${model} | null> {
          return db.${model.toLowerCase()}s.get(id)
        }
        
        async update(id: string, data: Partial<${model}>): Promise<${model}> {
          return db.${model.toLowerCase()}s.update(id, data)
        }
        
        async delete(id: string): Promise<void> {
          return db.${model.toLowerCase()}s.delete(id)
        }
      }
    `
  }
}
```

#### AST Transformation

```typescript
// macros/optimize.ts

import { CompileTimeMacro, MacroContext } from 'sinkdb/macros'
import * as ts from 'typescript'

export const inline_constants: CompileTimeMacro = {
  name: 'inline_constants',
  
  execute(ctx: MacroContext): void {
    const { ast } = ctx
    
    // Find constant calculations
    const visitor = (node: ts.Node): ts.Node => {
      if (ts.isBinaryExpression(node)) {
        // If both operands are literals, compute at compile time
        if (ts.isNumericLiteral(node.left) && ts.isNumericLiteral(node.right)) {
          const left = parseFloat(node.left.text)
          const right = parseFloat(node.right.text)
          
          let result: number
          switch (node.operatorToken.kind) {
            case ts.SyntaxKind.PlusToken:
              result = left + right
              break
            case ts.SyntaxKind.MinusToken:
              result = left - right
              break
            case ts.SyntaxKind.AsteriskToken:
              result = left * right
              break
            case ts.SyntaxKind.SlashToken:
              result = left / right
              break
            default:
              return node
          }
          
          return ts.factory.createNumericLiteral(result)
        }
      }
      
      return ts.visitEachChild(node, visitor, ctx.transformContext)
    }
    
    ctx.ast = ts.visitNode(ast, visitor)
  }
}
```

### Macro Registry

```typescript
// sinkdb.macros.ts (in your project)

import { MacroRegistry } from 'sinkdb/macros'
import * as gitInfo from './macros/git-info'
import * as validation from './macros/validation'
import * as performance from './macros/performance'
import * as crud from './macros/crud-generator'

export default MacroRegistry.create({
  compileTime: {
    git_hash: gitInfo.git_hash,
    build_timestamp: gitInfo.build_timestamp,
    semver: gitInfo.semver,
    generate_crud: crud.generate_crud,
    inline_constants: crud.inline_constants,
  },
  
  runtime: {
    validate: validation.validate,
    sanitize: validation.sanitize,
    memoize: performance.memoize,
    log_performance: performance.log_performance,
  }
})
```

### Configuration

```toml
# sinkdb.toml

[macros]
enabled = true
registry = "./sinkdb.macros.ts"

[macros.compile_time]
execute_on = "generate"
cache_results = true

[macros.runtime]
include_in_bundle = true
tree_shake = true
```

### CLI Integration

```bash
# List available macros
sinkdb macro list

# Compile-time macros:
#   git_hash            - Embed git commit hash
#   build_timestamp     - Embed build timestamp
#   semver              - Extract version from package.json
#
# Runtime macros:
#   validate            - Add validation decorator
#   memoize             - Cache function results
#   log_performance     - Log execution time

# Test macro
sinkdb macro test git_hash

# Output: a3f5c8d9e2b1f4c7a6d8e9f0a1b2c3d4e5f6a7b8

# Generate with specific macro
sinkdb generate --macro-only git_hash,build_timestamp
```

### Real-World Example: Release Signature

#### Schema

```
/**
 * @macro compile_time embed_release_signature
 */
ReleaseInfo {
  id: +uuid
  
  /** @macro compile_time git_hash */
  commit: char(40)
  
  /** @macro compile_time git_branch */
  branch: char(50)
  
  /** @macro compile_time build_timestamp */
  built_at: timestamp
  
  /** @macro compile_time semver */
  version: char(20)
  
  /** @macro compile_time build_environment */
  environment: char(20)
  
  /** @macro compile_time builder_info */
  builder: BuilderInfo
}

struct BuilderInfo {
  /** @macro compile_time hostname */
  machine: char(50)
  
  /** @macro compile_time username */
  user: char(30)
  
  /** @macro compile_time node_version */
  node: char(20)
  
  /** @macro compile_time rust_version */
  rust: char(20)
}
```

#### Macro Implementation

```typescript
// macros/release-signature.ts

import { CompileTimeMacro } from 'sinkdb/macros'
import { execSync } from 'child_process'
import os from 'os'

export const git_hash: CompileTimeMacro = {
  name: 'git_hash',
  execute: () => execSync('git rev-parse HEAD').toString().trim()
}

export const git_branch: CompileTimeMacro = {
  name: 'git_branch',
  execute: () => execSync('git rev-parse --abbrev-ref HEAD').toString().trim()
}

export const build_timestamp: CompileTimeMacro = {
  name: 'build_timestamp',
  execute: () => Date.now()
}

export const semver: CompileTimeMacro = {
  name: 'semver',
  execute: () => require('./package.json').version
}

export const build_environment: CompileTimeMacro = {
  name: 'build_environment',
  execute: () => process.env.NODE_ENV || 'development'
}

export const hostname: CompileTimeMacro = {
  name: 'hostname',
  execute: () => os.hostname()
}

export const username: CompileTimeMacro = {
  name: 'username',
  execute: () => os.userInfo().username
}

export const node_version: CompileTimeMacro = {
  name: 'node_version',
  execute: () => process.version
}

export const rust_version: CompileTimeMacro = {
  name: 'rust_version',
  execute: () => execSync('rustc --version').toString().trim().split(' ')[1]
}
```

#### Generated Code

```typescript
// generated/release-info.ts

export const RELEASE_INFO = {
  commit: "a3f5c8d9e2b1f4c7a6d8e9f0a1b2c3d4e5f6a7b8",
  branch: "main",
  built_at: 1697123456789,
  version: "1.2.3",
  environment: "production",
  builder: {
    machine: "build-server-01",
    user: "ci-user",
    node: "v20.9.0",
    rust: "1.75.0"
  }
} as const

// Type-safe access
export type ReleaseInfo = typeof RELEASE_INFO
```

#### Usage in UI

```jsx
// components/Footer.jsx

import { RELEASE_INFO } from '../generated/release-info'

export function Footer() {
  return (
    <footer className="app-footer">
      <div className="release-signature">
        <span>v{RELEASE_INFO.version}</span>
        <span title={`Commit: ${RELEASE_INFO.commit}`}>
          {RELEASE_INFO.commit.slice(0, 7)}
        </span>
        <span>{new Date(RELEASE_INFO.built_at).toLocaleDateString()}</span>
        
        {/* Click to see full details */}
        <details>
          <summary>Build Info</summary>
          <pre>{JSON.stringify(RELEASE_INFO, null, 2)}</pre>
        </details>
      </div>
    </footer>
  )
}
```

### Macro Context API

```typescript
// sinkdb/macros.d.ts

export interface MacroContext {
  // Schema information
  schema: Schema
  model?: Model
  field?: Field
  
  // AST (for compile-time macros)
  ast?: ts.SourceFile
  transformContext?: ts.TransformationContext
  
  // Configuration from @macro_config
  config: Record<string, any>
  
  // Build environment
  env: {
    mode: 'development' | 'production'
    target: 'native' | 'wasm'
    timestamp: number
  }
  
  // Utilities
  utils: {
    exec(cmd: string): string
    readFile(path: string): string
    writeFile(path: string, content: string): void
    getGitInfo(): GitInfo
    getPackageJson(): any
  }
}

export interface CompileTimeMacro {
  name: string
  execute(ctx: MacroContext): string | number | void
}

export interface RuntimeMacro {
  name: string
  generate(ctx: MacroContext): 
    | PropertyDecorator 
    | MethodDecorator 
    | ClassDecorator
}
```

### Benefits

1. **No new language**: Use TypeScript for macros
2. **Full power**: Access to Node APIs, filesystem, git, etc.
3. **Type-safe**: Macro implementations are type-checked
4. **Extensible**: Users can create custom macros
5. **Composable**: Stack multiple macros
6. **IDE support**: TypeScript tooling works
7. **Debuggable**: Step through macro code

### Security Considerations

```toml
[macros.security]
sandbox = true                    # Run in VM
allowed_imports = [               # Whitelist imports
  "child_process.execSync",
  "fs.readFileSync",
  "path.*"
]
network_access = false            # No network in macros
max_execution_time = 5000         # 5s timeout
```

### Performance

- **Compile-time macros**: No runtime cost (executed once during generation)
- **Runtime macros**: Minimal overhead (simple decorators)
- **Caching**: Macro results cached between builds

### Testing Macros

```typescript
// macros/git-info.test.ts

import { git_hash } from './git-info'
import { createMockContext } from 'sinkdb/testing'

describe('git_hash macro', () => {
  test('returns valid git hash', () => {
    const ctx = createMockContext()
    const hash = git_hash.execute(ctx)
    
    expect(hash).toMatch(/^[a-f0-9]{40}$/)
  })
  
  test('is deterministic in CI', () => {
    process.env.CI = 'true'
    const ctx = createMockContext()
    
    const hash1 = git_hash.execute(ctx)
    const hash2 = git_hash.execute(ctx)
    
    expect(hash1).toBe(hash2)
  })
})
```

---

## Open Questions

- **Tuple constraints**: How to validate relationships in tuples?
- **Blockchain performance**: Can we achieve <100ms block time?
- **WASM size**: Can we keep bundle < 2MB?
- **Sync bandwidth**: Acceptable for mobile networks?
- **Cache coherence**: How to handle hot cache in distributed setting?
- **Macro security**: How strict should sandboxing be? Allow filesystem access?
- **Macro composition**: How do multiple macros on same field interact?
- **Macro versioning**: How to handle breaking changes in macro APIs?

---

## Notes for Later Fleshing Out

### Blockchain Details Needed
- Consensus algorithm specifics
- Byzantine fault tolerance requirements
- Performance benchmarks vs traditional replication
- Privacy considerations (encrypted transactions?)
- Pruning strategy for old blocks

### WASM Live Sync Details Needed
- Binary protocol specification
- Subscription filtering language
- Bandwidth optimization strategies
- Battery/performance impact on mobile
- Progressive enhancement (graceful degradation)

### Macro System Details Needed
- Complete macro API documentation
- Standard library of built-in macros
- Best practices for macro composition
- Performance benchmarks (macro execution time)
- Security audit of macro sandbox
- Integration with existing TypeScript tooling
- Hot reload support for macro changes
- Macro debugging experience

---

**Document Version**: 0.2.0  
**Last Updated**: 2025-10-11  
**Status**: Feature Proposals & Ideas
