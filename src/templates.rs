/// Default blank schema template
pub fn blank_schema() -> &'static str {
    r#"// ForgeDB Schema
// Define your models below

User {
  id: +uuid
  email: ^&string @email
  created_at: +timestamp
}
"#
}

/// Blog template schema
pub fn blog_schema() -> &'static str {
    r#"// Blog Schema

User {
  id: +uuid
  username: ^&string
  email: ^&string @email
  password_hash: string
  created_at: +timestamp
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  slug: ^&string
  content: string
  published: bool
  published_at: timestamp?
  created_at: +timestamp
  updated_at: +timestamp
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  name: ^&string
  posts: [Post]
}
"#
}

/// E-commerce template schema
pub fn ecommerce_schema() -> &'static str {
    r#"// E-commerce Schema

User {
  id: +uuid
  email: ^&string @email
  name: string
  created_at: +timestamp
  orders: [Order]
}

Product {
  id: +uuid
  sku: ^&string
  name: string
  description: string
  price: f64 @min(0)
  stock: u32
  created_at: +timestamp
}

Order {
  id: +uuid
  customer: *User
  status: string
  total: f64
  created_at: +timestamp
  items: [OrderItem]
}

OrderItem {
  id: +uuid
  order: *Order
  product: *Product
  quantity: u32 @min(1)
  price: f64
}
"#
}

/// Todo app template schema
pub fn todo_schema() -> &'static str {
    r#"// Todo App Schema

User {
  id: +uuid
  email: ^&string @email
  name: string
  created_at: +timestamp
  todos: [Todo]
}

Todo {
  id: +uuid
  title: string
  description: string?
  completed: bool
  due_date: timestamp?
  created_at: +timestamp
  updated_at: +timestamp
  owner: *User
}
"#
}

/// Default forgedb.toml configuration.
///
/// `isolated` is written **explicitly, always** (#333). The field's absent value
/// is `true`, so writing it changes nothing today — it exists so that "these are
/// separate projects" is a declaration rather than an absence. An absence is
/// fragile: someone adds a config above this one for an unrelated reason and
/// every app beneath it silently regroups into one build cache.
pub fn default_config(project_name: &str, isolated: bool) -> String {
    // `name` is written ONLY when this config is a project root. A nested,
    // non-isolated config that declares a name is a contradiction ForgeDB rejects
    // (#333 §6) — it reads as authoritative and is not — so scaffolding one would
    // make `init` emit a config that fails on the very next `generate`.
    let name = if isolated {
        format!("name = \"{project_name}\"\n")
    } else {
        "# No `name`: this config joins the enclosing project, and only a project root\n\
         # may name one. Set `isolated = true` below to make this a project of its own.\n"
            .to_string()
    };
    format!(
        r#"[project]
{}version = "0.1.0"
# Do the schemas beneath this config form their own project, or do they join an
# enclosing one? A project is the unit of build cache, lockfile and target dir.
isolated = {}

# Generator configuration — consumed by the forgedb CLI.
# Precedence: explicit CLI flag > values below > built-in defaults.
[generate]
# NOTE: there is no `schema` key. A config governs every schema beneath it, so it
# cannot name one — pass `--schema`, or keep the schema next to this file.
# Relative paths below resolve against the SCHEMA's directory, so under a shared
# root config `output` is a per-app pattern rather than one shared directory.
output = "generated"
# Which targets to generate. REQUIRED (#335) — an absent value used to mean
# "everything", which is the opposite of what an absent list normally reads as.
# `all` means every target; name them individually to narrow it. The spellings
# are the same ones the CLI takes: node-sdk, node-runtime, python-runtime,
# go-runtime, browser-replica, rust, api, openapi, stubs, ffi, *-sdk.
targets = ["all"]

# Generate-time RUNTIME BEHAVIOR (epic #126). These schema-blind knobs are baked
# into the generated database.rs when you run `forgedb generate` / `forgedb build`
# — the Rust compiler optimizes around them (there is no runtime settings engine).
# Every value below is the DEFAULT; the commented lines document what you can
# change. Changing one requires regenerating.
[runtime]
# Attach the durable replication broker so `/replicate` and browser read-replica
# followers work (#130). OFF by default: an unused broker fsyncs a SECOND time per
# write (pure waste when nothing is subscribed). Turn on only if you consume it.
replication = false
# In-process change-feed / durable-broker buffer capacity (#135).
# changefeed_capacity = 1024
# Max @on_delete(cascade) recursion depth — a structural safety bound (#150).
# max_cascade_depth = 64
# Durable replication-log retention (#137): when replication is on, maintain()
# prunes _replication.log to the last N broker offsets. 0 = keep the full log
# (default). A follower resuming from an offset older than the retained window
# must re-seed from a fresh backup.
# replication_log_retention = 0

# Generate-time STORAGE / DURABILITY knobs (epic #126). Also baked at generate
# time. Defaults are crash-safe and byte-identical to prior releases.
[storage]
# WAL fsync policy (#129): "always" (default — every acked write is crash-safe)
# or "never" (OS flushes on its own schedule; FASTER but a real data-loss window
# on power loss — an explicit, deliberate durability trade-off).
# fsync = "always"
# Mutations per collection between WAL checkpoints (#131) — bounds WAL growth.
# wal_checkpoint_interval = 1000
# In-process auto-compaction of dead row versions (#134). true = reclaim
# automatically; false = omit the auto-trigger (reclaim only via an explicit
# Database::compact() call).
# compaction = true
# Dead row versions per model before auto-compaction fires (#133).
# compaction_threshold = 1000

# Generate-time TRANSACTION knobs (epic #126). Baked at generate time.
[transaction]
# Default retry count for `transaction_optimistic` (#146) — a conflict-losing
# optimistic transaction is re-run up to max_retries + 1 times. Per-call control
# stays available via `transaction_retrying(retries, f)`.
# max_retries = 3

# Generate-time SERVER / transport knobs (epic #126). Baked at generate time.
# (Host/port, log level/format, and shutdown drain are separate DEPLOY-env knobs
# read from the environment at process start — see the note below.)
[server]
# List-endpoint page size when the client omits ?limit (#141).
# page_default_limit = 50
# Maximum list-endpoint page size; a larger ?limit is clamped (#141).
# page_max_limit = 1000
# Emit the unauthenticated /metrics endpoint (#151). false omits the handler +
# route entirely (/health, /ready, /snapshot are unaffected).
# metrics = true

# Generate-time WASM read-replica knobs (epic #126). Substituted into the static,
# schema-agnostic Worker bootstrap at generate time.
[wasm]
# Browser read-replica auto-commit debounce, ms (#148) — trades persist-write
# frequency vs the tab-close loss window.
# commit_debounce_ms = 250
# Commit immediately once this many replication frames are buffered (#148).
# commit_max_frames = 100

# Note: the generated server's host/port, log level/format, and graceful-shutdown
# drain timeout are DEPLOY-environment config, read from the environment at
# process start (FORGEDB_HOST / FORGEDB_PORT / RUST_LOG / FORGEDB_LOG_FORMAT /
# FORGEDB_SHUTDOWN_TIMEOUT) — 12-factor, not baked into the binary. See the
# generated main.rs and docs/DEPLOYMENT.md.

# Multi-tenancy (#59): physical, dir-per-tenant isolation. Each tenant's data
# lives under <root>/<tenant>/, and ONE `forgedb serve` process serves ONE
# tenant (process-per-tenant). Manage tenant dirs with `forgedb tenant`.
[tenant]
root = "./data"

# Verify-only JWT tenant guard for the generated server (#59). ForgeDB verifies
# tokens signed by YOUR identity provider (Auth0/Clerk/Cognito/…) — it never
# issues tokens. The generated process reads these at runtime via env
# (FORGEDB_TENANT selects which tenant this process serves); this table is the
# declarative source. Off by default.
[auth]
enabled = false
# issuer = "https://your-idp.example/"
# audience = "your-api"
# tenant_claim = "tenant"          # claim carrying the tenant id (default)
# jwks_url = "https://your-idp.example/.well-known/jwks.json"
# jwks_refresh_secs = 300          # JWKS re-fetch interval for key rotation (#81)
# public_key_path = "./idp_pub.pem"  # alternative to jwks_url (static key)
# algorithms = ["RS256"]           # asymmetric-only allowlist; multiple allowed,
#                                  # e.g. ["RS256", "ES256"] (default ["RS256"])
# leeway_secs = 60                 # clock-skew leeway seconds (default)
"#,
        name, isolated
    )
}

/// Default .gitignore
///
/// **Generated TEXT is committed; only compiled output is ignored** (#335 §15).
/// This file used to ignore `/generated/` wholesale, which contradicted the rule
/// outright — and after #335 it is plainly wrong: ForgeDB compiles the generated
/// Rust in its own cache under `$FORGEDB_HOME`, so nothing under `generated/` is
/// a build artifact any more. It holds reviewable, diffable source.
pub fn default_gitignore() -> &'static str {
    r#"# ForgeDB generated code is COMMITTED, not ignored.
#
# `generated/` holds source you review and ship — database.rs, api.rs, types.ts,
# openapi.json, the client SDKs and go/. ForgeDB compiles the Rust in its own
# build cache under $FORGEDB_HOME, so none of this is build output. Commit it.
#
# The one COMPILED artifact ForgeDB delivers into the tree is the static library
# the Go binding links against, and that is ignored:
/generated/**/*.a
/generated/**/*.lib

# Database Files
/data/

# Rust — ForgeDB scaffolds no cargo package (#335); these cover a crate you add
# yourself. Cargo.lock is deliberately NOT ignored: a binary crate commits its
# lockfile, and ForgeDB's own lockfile lives in its cache, not here.
/target/
**/*.rs.bk

# TypeScript / Node
node_modules/
dist/
*.js
*.d.ts
!vite.config.js

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db
"#
}

/// README template
pub fn readme_template(project_name: &str) -> String {
    format!(
        r#"# {}

A ForgeDB project.

## Getting Started

### Development

```bash
# Generate code from schema.forge
forgedb generate

# Compile it (ForgeDB builds in its own cache — there is no Cargo.toml here)
forgedb build

# Where did the server binary land?
forgedb build --release --print-artifact server
```

### Schema

Edit `schema.forge` to define your data models, then re-run `forgedb generate`.
Which generators run is declared by `targets` under `[generate]` in
`forgedb.toml`.

### Generated Code

`generated/` is source: review it, diff it, commit it. ForgeDB compiles the Rust
in its own build cache under `$FORGEDB_HOME`, so this directory holds no build
output.

- `generated/database.rs` - Rust database implementation (a read-only mirror of
  the copy ForgeDB compiles)
- `generated/api.rs` - the REST API layer
- `generated/types.ts`, `generated/openapi.json` - the TypeScript types and the
  OpenAPI document

### Deploying

`Dockerfile` / `docker-compose.yml` build the image by driving the CLI, and
`deploy/` holds the on-host systemd path. Both copy out the artifact path
`forgedb build` reports rather than constructing one.

## Learn More

- [ForgeDB Documentation](https://github.com/yourusername/forgedb)
- [Schema Language Guide](https://github.com/yourusername/forgedb/blob/main/docs/schema.md)
"#,
        project_name
    )
}
