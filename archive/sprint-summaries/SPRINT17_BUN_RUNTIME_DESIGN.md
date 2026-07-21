# Sprint 17: Bun Runtime Integration - Technical Design

**Status**: Design Phase
**Created**: 2025-10-14
**Sprint Goal**: Integrate Bun runtime for React SSR component rendering with efficient process synchronization

---

## Architecture Overview

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │
       ▼
┌─────────────────────────────┐
│  Axum Reverse Proxy         │
│  (Rust, Port 8080)          │
└──┬────────────────────────┬─┘
   │                        │
   │ /api/*                 │ /components/*
   ▼                        ▼
┌────────────┐          ┌─────────────┐
│ Rust API   │          │ Bun Server  │
│ (Port 3000)│          │ (Port 3001) │
└─────┬──────┘          └──────┬──────┘
      │                        │
      │  Unix Socket           │ Read-Only
      │  + Shared Memory       │ Access
      └────────┬───────────────┘
               │
               ▼
        ┌──────────────┐
        │ ForgeDB      │
        │ Data Files   │
        └──────────────┘
```

---

## Component 1: Process Synchronization

### Problem Statement

Two processes (Rust API server and Bun component server) need to access the same database files:
- **Rust**: Read/write access, owns the database
- **Bun**: Read-only access, renders components with data

**Requirements**:
- Bun must see consistent snapshots (no partial writes)
- Minimal latency (< 10μs notification)
- Zero CPU overhead when idle
- No network calls between processes

### Solution: Hybrid eventfd + Shared Memory

**Key Insight**: Combine the speed of shared memory with the efficiency of event-driven notifications.

#### Performance Characteristics

- **Write notification**: ~1-2μs (eventfd signal)
- **Read notification**: 0% CPU when idle (blocking wait)
- **Version check**: ~50ns (shared memory read)
- **Total overhead per transaction**: < 2μs

---

## Implementation Details

### 1. Shared Memory Structure

```rust
// src/ipc/shared_state.rs

use memmap2::MmapMut;
use std::fs::OpenOptions;
use std::sync::Arc;

/// Shared memory layout:
/// [0..8]   - Version counter (u64, little-endian)
/// [8..16]  - Timestamp of last update (i64, unix timestamp)
/// [16..24] - Reserved for future use
pub struct SharedState {
    mmap: MmapMut,
    path: String,
}

impl SharedState {
    pub fn new(path: &str) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        
        // Allocate 64 bytes for future expansion
        file.set_len(64)?;
        
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        
        Ok(Self {
            mmap,
            path: path.to_string(),
        })
    }
    
    pub fn write_version(&mut self, version: u64) {
        let bytes = version.to_le_bytes();
        self.mmap[0..8].copy_from_slice(&bytes);
        
        // Write timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let ts_bytes = timestamp.to_le_bytes();
        self.mmap[8..16].copy_from_slice(&ts_bytes);
        
        // Ensure writes are visible to other processes
        self.mmap.flush()?;
    }
    
    pub fn read_version(&self) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.mmap[0..8]);
        u64::from_le_bytes(bytes)
    }
    
    pub fn read_timestamp(&self) -> i64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.mmap[8..16]);
        i64::from_le_bytes(bytes)
    }
}
```

### 2. Event Notification (Unix Socket Alternative)

**Note**: Using Unix socket instead of eventfd for better cross-platform support and simpler implementation.

```rust
// src/ipc/notifier.rs

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct EventNotifier {
    listener: UnixListener,
    path: String,
    clients: Arc<Mutex<Vec<UnixStream>>>,
}

impl EventNotifier {
    pub fn new(socket_path: &str) -> Result<Self> {
        // Remove old socket if exists
        let _ = std::fs::remove_file(socket_path);
        
        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;
        
        let clients = Arc::new(Mutex::new(Vec::new()));
        
        // Spawn task to accept new connections
        let clients_clone = clients.clone();
        let listener_clone = listener.try_clone()?;
        tokio::spawn(async move {
            Self::accept_connections(listener_clone, clients_clone).await;
        });
        
        Ok(Self {
            listener,
            path: socket_path.to_string(),
            clients,
        })
    }
    
    async fn accept_connections(
        listener: UnixListener,
        clients: Arc<Mutex<Vec<UnixStream>>>,
    ) {
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    clients.lock().await.push(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                    break;
                }
            }
        }
    }
    
    pub async fn notify(&self, version: u64) -> Result<()> {
        let mut clients = self.clients.lock().await;
        let message = version.to_le_bytes();
        
        // Send to all connected clients, remove dead ones
        clients.retain(|stream| {
            use std::io::Write;
            stream.write_all(&message).is_ok()
        });
        
        Ok(())
    }
}

impl Drop for EventNotifier {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
```

### 3. Sync Manager (Combines Both)

```rust
// src/ipc/sync_manager.rs

use crate::ipc::{SharedState, EventNotifier};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SyncManager {
    shared_state: Arc<RwLock<SharedState>>,
    notifier: Arc<EventNotifier>,
    current_version: Arc<RwLock<u64>>,
}

impl SyncManager {
    pub fn new(shm_path: &str, socket_path: &str) -> Result<Self> {
        let shared_state = SharedState::new(shm_path)?;
        let notifier = EventNotifier::new(socket_path)?;
        
        let current_version = shared_state.read_version();
        
        Ok(Self {
            shared_state: Arc::new(RwLock::new(shared_state)),
            notifier: Arc::new(notifier),
            current_version: Arc::new(RwLock::new(current_version)),
        })
    }
    
    /// Called after each transaction commit
    pub async fn notify_data_changed(&self) -> Result<()> {
        // Increment version
        let mut version = self.current_version.write().await;
        *version += 1;
        
        // Write to shared memory
        let mut state = self.shared_state.write().await;
        state.write_version(*version);
        drop(state);
        
        // Notify all Bun processes (~1-2μs)
        self.notifier.notify(*version).await?;
        
        Ok(())
    }
    
    pub async fn get_version(&self) -> u64 {
        *self.current_version.read().await
    }
    
    pub fn socket_path(&self) -> &str {
        &self.notifier.path
    }
    
    pub fn shm_path(&self) -> String {
        self.shared_state.read().await.path.clone()
    }
}
```

### 4. Integration with Database

```rust
// src/database/mod.rs

impl Database {
    pub async fn commit_transaction(&mut self, tx: Transaction) -> Result<()> {
        // 1. Write to WAL
        self.wal.append(&tx)?;
        
        // 2. Apply changes to database
        self.apply_transaction(&tx)?;
        
        // 3. Flush to disk
        self.wal.sync()?;
        
        // 4. Notify Bun processes (~1-2μs overhead)
        if let Some(sync_mgr) = &self.sync_manager {
            sync_mgr.notify_data_changed().await?;
        }
        
        Ok(())
    }
}
```

---

## Component 2: Bun Server Implementation

### 1. Database Synchronization Client

```typescript
// bun-server/src/db-sync.ts

import { connect } from "bun";
import { readFileSync } from "fs";

export class DBSync {
  private socket: any;
  private shm: Buffer;
  private currentVersion: bigint = 0n;
  private listeners: Set<(version: bigint) => void> = new Set();
  
  constructor(
    private shmPath: string,
    private socketPath: string
  ) {}
  
  async connect(): Promise<void> {
    // Open shared memory for reading
    const shmFile = Bun.file(this.shmPath);
    const shmData = await shmFile.arrayBuffer();
    this.shm = Buffer.from(shmData);
    
    // Connect to Unix socket
    this.socket = await connect({
      unix: this.socketPath,
    });
    
    // Start listening for updates
    this.startListening();
  }
  
  private async startListening() {
    try {
      for await (const chunk of this.socket) {
        // Read version from notification (8 bytes, little-endian u64)
        const buffer = Buffer.from(chunk);
        const newVersion = buffer.readBigUInt64LE(0);
        
        if (newVersion > this.currentVersion) {
          console.log(`[DBSync] Version updated: ${this.currentVersion} → ${newVersion}`);
          this.currentVersion = newVersion;
          
          // Notify all listeners
          for (const listener of this.listeners) {
            listener(newVersion);
          }
        }
      }
    } catch (error) {
      console.error("[DBSync] Connection lost:", error);
      // Implement reconnection logic
      await this.reconnect();
    }
  }
  
  private async reconnect() {
    console.log("[DBSync] Attempting to reconnect...");
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    try {
      await this.connect();
      console.log("[DBSync] Reconnected successfully");
    } catch (error) {
      console.error("[DBSync] Reconnection failed:", error);
      await this.reconnect();
    }
  }
  
  getCurrentVersion(): bigint {
    return this.currentVersion;
  }
  
  // Register callback for version changes
  onVersionChange(callback: (version: bigint) => void) {
    this.listeners.add(callback);
    return () => this.listeners.delete(callback);
  }
  
  // Read version directly from shared memory (for verification)
  readVersionFromShm(): bigint {
    const shmData = readFileSync(this.shmPath);
    return shmData.readBigUInt64LE(0);
  }
}
```

### 2. Component Rendering Server

```typescript
// bun-server/src/server.ts

import { renderToReadableStream } from "react-dom/server";
import { DBSync } from "./db-sync";
import { components } from "./components";
import { createDBClient } from "./db-client";

// Initialize DB sync
const dbSync = new DBSync(
  process.env.FORGEDB_SHM || "/tmp/forgedb.shm",
  process.env.FORGEDB_SOCKET || "/tmp/forgedb.sock"
);

await dbSync.connect();

// Create DB client (calls Rust API for Sprint 17)
const db = createDBClient({
  apiEndpoint: process.env.RUST_API_URL || "http://localhost:3000",
});

// Simple in-memory cache
const cache = new Map<string, { data: any; version: bigint }>();

// Invalidate cache on version change
dbSync.onVersionChange((version) => {
  console.log(`[Cache] Clearing cache due to version ${version}`);
  cache.clear();
});

// Component rendering server
Bun.serve({
  port: process.env.PORT || 3001,
  
  async fetch(req: Request): Promise<Response> {
    const url = new URL(req.url);
    
    // Only handle /components/* routes
    if (!url.pathname.startsWith("/components/")) {
      return new Response("Not Found", { status: 404 });
    }
    
    try {
      // Parse route: /components/User/card/123
      const parts = url.pathname.split("/").filter(Boolean);
      if (parts.length !== 4) {
        return new Response("Invalid component path", { status: 400 });
      }
      
      const [_, modelName, componentName, id] = parts;
      
      // Check cache
      const cacheKey = `${modelName}:${componentName}:${id}`;
      const currentVersion = dbSync.getCurrentVersion();
      const cached = cache.get(cacheKey);
      
      if (cached && cached.version === currentVersion) {
        console.log(`[Cache] Hit: ${cacheKey}`);
        return new Response(cached.data, {
          headers: { "Content-Type": "text/html" },
        });
      }
      
      // Fetch data from database
      const data = await db.get(modelName, id);
      if (!data) {
        return new Response("Not Found", { status: 404 });
      }
      
      // Get component
      const componentKey = `${modelName}${componentName.charAt(0).toUpperCase() + componentName.slice(1)}`;
      const Component = components[componentKey];
      
      if (!Component) {
        return new Response(`Component ${componentKey} not found`, { status: 404 });
      }
      
      // Render component
      const stream = await renderToReadableStream(
        <Component data={data} />,
        {
          bootstrapScripts: ["/static/hydrate.js"],
        }
      );
      
      // Cache the response
      const html = await Bun.readableStreamToText(stream);
      cache.set(cacheKey, { data: html, version: currentVersion });
      
      return new Response(html, {
        headers: {
          "Content-Type": "text/html",
          "X-ForgeDB-Version": currentVersion.toString(),
        },
      });
      
    } catch (error) {
      console.error("[Render] Error:", error);
      return new Response("Internal Server Error", { status: 500 });
    }
  },
});

console.log(`🎨 Bun component server running on port ${process.env.PORT || 3001}`);
console.log(`📊 DB version: ${dbSync.getCurrentVersion()}`);
```

### 3. Database Client (Stub for Sprint 17)

**Note**: This is a temporary implementation for Sprint 17. Direct ForgeDB access via FFI will be implemented in Sprint 24.

```typescript
// bun-server/src/db-client.ts

export interface DBClientConfig {
  apiEndpoint: string; // Rust API server URL
}

export function createDBClient(config: DBClientConfig) {
  // For Sprint 17: Call Rust API endpoints
  // For Sprint 24: Direct ForgeDB access via FFI

  return {
    async get(model: string, id: string): Promise<any> {
      const response = await fetch(
        `${config.apiEndpoint}/api/${model.toLowerCase()}/${id}`
      );

      if (!response.ok) {
        throw new Error(`Failed to fetch ${model}:${id}`);
      }

      return response.json();
    },

    async query(model: string, filters: Record<string, any>): Promise<any[]> {
      const queryParams = new URLSearchParams(filters as any);
      const response = await fetch(
        `${config.apiEndpoint}/api/${model.toLowerCase()}?${queryParams}`
      );

      if (!response.ok) {
        throw new Error(`Failed to query ${model}`);
      }

      return response.json();
    },

    // For Sprint 24: Will use FFI to access ForgeDB directly
    // import { Database } from "./ffi/forgedb";
    // const db = new Database(config.dataPath, { readOnly: true });
  };
}
```

### 4. Component Registry

```typescript
// bun-server/src/components/index.ts

import UserCard from "./UserCard";
import UserProfile from "./UserProfile";
import PostCard from "./PostCard";

export const components = {
  UserCard,
  UserProfile,
  PostCard,
  // Auto-generated from schema
};
```

---

## Component 3: Axum Reverse Proxy

### Implementation

```rust
// src/proxy/mod.rs

use axum::{
    Router,
    extract::Request,
    response::{Response, IntoResponse},
    http::StatusCode,
};
use hyper::body::Incoming;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use tower::ServiceBuilder;
use std::time::Duration;

pub struct ProxyConfig {
    pub rust_api_addr: String,
    pub bun_server_addr: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            rust_api_addr: "http://127.0.0.1:3000".to_string(),
            bun_server_addr: "http://127.0.0.1:3001".to_string(),
        }
    }
}

pub fn create_proxy(config: ProxyConfig) -> Router {
    Router::new()
        .fallback(move |req: Request| proxy_handler(req, config.clone()))
}

async fn proxy_handler(
    mut req: Request,
    config: ProxyConfig,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    
    // Determine backend based on path
    let backend = if path.starts_with("/components/") {
        &config.bun_server_addr
    } else {
        &config.rust_api_addr
    };
    
    // Build new URI
    let uri_string = format!(
        "{}{}",
        backend,
        req.uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(path)
    );
    
    // Parse new URI
    let uri = uri_string
        .parse::<hyper::Uri>()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Update request URI
    *req.uri_mut() = uri;
    
    // Create HTTP client with connection pooling
    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build_http();
    
    // Forward request
    match client.request(req).await {
        Ok(response) => Ok(response.into_response()),
        Err(e) => {
            eprintln!("Proxy error: {}", e);
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

pub async fn start_proxy(
    config: ProxyConfig,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_proxy(config);
    
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await?;
    
    println!("🔀 Reverse proxy listening on port {}", port);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
```

---

## Component 4: Application Orchestration

### Main Entry Point

```rust
// src/main.rs

use forgedb::{Database, SyncManager};
use std::process::Command;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🚀 Starting ForgeDB...");
    
    // 1. Open database
    let db = Database::open("./data")?;
    println!("✅ Database opened");
    
    // 2. Initialize sync manager
    let sync_mgr = SyncManager::new(
        "/tmp/forgedb.shm",
        "/tmp/forgedb.sock",
    )?;
    println!("✅ Sync manager initialized");
    
    // 3. Start Bun component server
    let bun_handle = tokio::spawn(async {
        start_bun_server().await
    });
    
    // Wait for Bun to start
    sleep(Duration::from_millis(500)).await;
    println!("✅ Bun server started on port 3001");
    
    // 4. Start Rust API server
    let api_handle = tokio::spawn(async move {
        start_api_server(db, sync_mgr).await
    });
    println!("✅ API server started on port 3000");
    
    // 5. Start reverse proxy
    let proxy_handle = tokio::spawn(async {
        let config = ProxyConfig::default();
        start_proxy(config, 8080).await
    });
    println!("✅ Reverse proxy started on port 8080");
    
    println!("\n🎉 ForgeDB is ready!");
    println!("   API: http://localhost:8080/api");
    println!("   Components: http://localhost:8080/components");
    
    // Wait for all servers
    tokio::try_join!(api_handle, bun_handle, proxy_handle)?;
    
    Ok(())
}

async fn start_bun_server() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("bun")
        .arg("run")
        .arg("bun-server/src/server.ts")
        .env("FORGEDB_SHM", "/tmp/forgedb.shm")
        .env("FORGEDB_SOCKET", "/tmp/forgedb.sock")
        .env("FORGEDB_DATA", "./data")
        .env("PORT", "3001")
        .spawn()?;
    
    let status = child.wait()?;
    
    if !status.success() {
        eprintln!("Bun server exited with error: {:?}", status);
    }
    
    Ok(())
}

async fn start_api_server(
    db: Database,
    sync_mgr: SyncManager,
) -> Result<(), Box<dyn std::error::Error>> {
    use axum::{Router, routing::get};
    
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        // ... add API routes
        .with_state((db, sync_mgr));
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await?;
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
```

---

## Testing Strategy

### 1. Unit Tests

```rust
// tests/sync_manager_test.rs

#[tokio::test]
async fn test_sync_manager_notification() {
    let sync_mgr = SyncManager::new(
        "/tmp/test_forgedb.shm",
        "/tmp/test_forgedb.sock",
    ).unwrap();
    
    let initial_version = sync_mgr.get_version().await;
    
    // Simulate transaction commit
    sync_mgr.notify_data_changed().await.unwrap();
    
    let new_version = sync_mgr.get_version().await;
    assert_eq!(new_version, initial_version + 1);
}
```

### 2. Integration Tests

```typescript
// bun-server/tests/sync.test.ts

import { test, expect } from "bun:test";
import { DBSync } from "../src/db-sync";

test("DBSync receives version updates", async () => {
  const dbSync = new DBSync("/tmp/test_forgedb.shm", "/tmp/test_forgedb.sock");
  await dbSync.connect();
  
  let receivedVersion = 0n;
  dbSync.onVersionChange((version) => {
    receivedVersion = version;
  });
  
  // Simulate Rust updating version (in separate process)
  // ... trigger update ...
  
  // Wait for notification
  await new Promise(resolve => setTimeout(resolve, 100));
  
  expect(receivedVersion).toBeGreaterThan(0n);
});
```

### 3. Performance Benchmarks

```rust
// benches/sync_benchmark.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_notification(c: &mut Criterion) {
    let sync_mgr = SyncManager::new(
        "/tmp/bench_forgedb.shm",
        "/tmp/bench_forgedb.sock",
    ).unwrap();
    
    c.bench_function("notify_data_changed", |b| {
        b.iter(|| {
            black_box(sync_mgr.notify_data_changed());
        });
    });
}

criterion_group!(benches, bench_notification);
criterion_main!(benches);
```

---

## Deployment Considerations

### 1. Process Management

Use systemd or similar to manage processes:

```ini
# /etc/systemd/system/forgedb.service
[Unit]
Description=ForgeDB Server
After=network.target

[Service]
Type=simple
User=forgedb
WorkingDirectory=/opt/forgedb
ExecStart=/opt/forgedb/forgedb
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### 2. Resource Limits

```rust
// Set rlimits for shared memory
use libc::{rlimit, setrlimit, RLIMIT_MEMLOCK};

pub fn increase_memlock_limit() -> Result<()> {
    let limit = rlimit {
        rlim_cur: 64 * 1024 * 1024, // 64MB
        rlim_max: 64 * 1024 * 1024,
    };
    
    unsafe {
        if setrlimit(RLIMIT_MEMLOCK, &limit) != 0 {
            return Err("Failed to set RLIMIT_MEMLOCK".into());
        }
    }
    
    Ok(())
}
```

### 3. Monitoring

```rust
// src/metrics/mod.rs

use prometheus::{Counter, Histogram, Registry};

pub struct Metrics {
    pub sync_notifications: Counter,
    pub sync_latency: Histogram,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Self {
        let sync_notifications = Counter::new(
            "forgedb_sync_notifications_total",
            "Total number of sync notifications sent"
        ).unwrap();
        
        let sync_latency = Histogram::new(
            "forgedb_sync_latency_seconds",
            "Time to send sync notification"
        ).unwrap();
        
        registry.register(Box::new(sync_notifications.clone())).unwrap();
        registry.register(Box::new(sync_latency.clone())).unwrap();
        
        Self {
            sync_notifications,
            sync_latency,
        }
    }
}
```

---

## Performance Targets

| Metric | Target | Expected |
|--------|--------|----------|
| Sync notification latency | < 10μs | ~1-2μs |
| Proxy overhead | < 100μs | ~10-50μs |
| Component render time | < 50ms | ~10-30ms |
| Cache hit rate | > 80% | ~90% |
| Idle CPU (Bun) | < 1% | ~0.1% |
| Memory overhead (shared) | < 1MB | ~64KB |

---

## Future Optimizations

### 1. Zero-Copy Rendering

Instead of reading from SQLite, use mmap directly:

```typescript
const data = mmap.read(offset, length);
// Parse and render without copying
```

### 2. Persistent Cache

```typescript
// Use SQLite or RocksDB for cache persistence
const cache = new Database("cache.db");
```

### 3. Multi-threaded Bun

```typescript
// Use Bun workers for parallel rendering
const worker = new Worker("renderer.ts");
worker.postMessage({ component, data });
```

### 4. HTTP/2 Push

```rust
// Push component assets with HTTP/2
response.push("/static/styles.css");
response.push("/static/hydrate.js");
```

---

## Security Considerations

### 1. File Permissions

```bash
# Shared memory and socket should be owned by forgedb user
chown forgedb:forgedb /tmp/forgedb.shm
chown forgedb:forgedb /tmp/forgedb.sock
chmod 600 /tmp/forgedb.shm
chmod 600 /tmp/forgedb.sock
```

### 2. Process Isolation

```rust
// Drop privileges after binding to port
use nix::unistd::{setuid, setgid};

fn drop_privileges() -> Result<()> {
    let uid = Uid::from_raw(1000); // forgedb user
    let gid = Gid::from_raw(1000);
    
    setgid(gid)?;
    setuid(uid)?;
    
    Ok(())
}
```

### 3. Input Validation

```typescript
// Validate component routes
function isValidComponentPath(path: string): boolean {
  const pattern = /^\/components\/[A-Za-z]+\/[a-z]+\/[a-f0-9-]+$/;
  return pattern.test(path);
}
```

---

## Rollout Plan

### Phase 1: Core Infrastructure (Week 1)
- [ ] Implement SharedState
- [ ] Implement EventNotifier
- [ ] Implement SyncManager
- [ ] Unit tests for all components

### Phase 2: Bun Integration (Week 2)
- [ ] Implement DBSync client
- [ ] Create component rendering server
- [ ] Implement read-only DB client
- [ ] Basic cache implementation

### Phase 3: Proxy & Orchestration (Week 3)
- [ ] Implement axum reverse proxy
- [ ] Create startup orchestration
- [ ] Integration tests
- [ ] Performance benchmarks

### Phase 4: Polish & Documentation (Week 4)
- [ ] Error handling & recovery
- [ ] Monitoring & metrics
- [ ] Documentation
- [ ] Example applications

---

## Success Criteria

- ✅ Sync notification latency < 10μs
- ✅ Zero CPU usage when idle
- ✅ Component renders in < 50ms
- ✅ No data races or undefined behavior
- ✅ Clean process lifecycle (startup/shutdown)
- ✅ Comprehensive test coverage (>80%)
- ✅ Production-ready error handling

---

## References

- [Bun Documentation](https://bun.sh/docs)
- [React renderToReadableStream](https://react.dev/reference/react-dom/server/renderToReadableStream)
- [Axum Web Framework](https://docs.rs/axum)
- [Unix Domain Sockets](https://man7.org/linux/man-pages/man7/unix.7.html)
- [mmap(2)](https://man7.org/linux/man-pages/man2/mmap.2.html)

---

**Document Version**: 1.0
**Last Updated**: 2025-10-14
**Status**: Ready for Implementation
