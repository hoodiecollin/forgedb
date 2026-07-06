# ForgeDB Development Guide

**Last Updated:** October 2025

Complete guide for setting up your development environment and working with ForgeDB.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Setting Up Development Environment](#setting-up-development-environment)
- [Building the Project](#building-the-project)
- [Running Tests](#running-tests)
- [Local Testing](#local-testing)
- [Debugging Tips](#debugging-tips)
- [IDE Configuration](#ide-configuration)
- [Common Development Tasks](#common-development-tasks)

---

## Prerequisites

### Required Tools

**Rust Toolchain:**
```bash
# Install rustup (Rust installer and version manager)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version  # Should be 1.70+
cargo --version
```

**Git:**
```bash
# Verify git installation
git --version  # Should be 2.0+
```

### Optional but Recommended

**Development Tools:**
```bash
# Cargo watch (auto-rebuild on file changes)
cargo install cargo-watch

# Cargo edit (manage dependencies from CLI)
cargo install cargo-edit

# Cargo expand (expand macros)
cargo install cargo-expand

# Flamegraph (performance profiling)
cargo install flamegraph

# Tarpaulin (code coverage)
cargo install cargo-tarpaulin
```

**System Dependencies:**
```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# macOS (with Homebrew)
brew install openssl pkg-config

# Fedora/RHEL
sudo dnf install gcc openssl-devel pkgconfig
```

---

## Setting Up Development Environment

### 1. Clone Repository

```bash
# Clone the repository
git clone https://github.com/yourusername/forgedb.git
cd forgedb

# Or use SSH
git clone git@github.com:yourusername/forgedb.git
cd forgedb
```

### 2. Verify Setup

```bash
# Check Rust installation
rustc --version

# Check cargo
cargo --version

# List workspace members
cargo metadata --format-version 1 | jq '.workspace_members'
```

### 3. Initial Build

```bash
# Build all crates in workspace
cargo build

# This will:
# - Download dependencies
# - Compile all crates
# - Create binaries in target/debug/
```

**Expected output:**
```
   Compiling forgedb-types v0.1.0 (/path/to/forgedb/crates/types)
   Compiling forgedb-parser v0.1.0 (/path/to/forgedb/crates/parser)
   ...
   Compiling forgedb v0.1.0 (/path/to/forgedb)
    Finished dev [unoptimized + debuginfo] target(s) in 2m 30s
```

### 4. Verify Installation

```bash
# Run tests to verify everything works
cargo test --lib

# Build documentation
cargo doc --no-deps --open
```

---

## Building the Project

### Build Commands

**Development Build (Fast, with debug info):**
```bash
# Build all crates
cargo build

# Build specific crate
cargo build --package forgedb-storage

# Build with all features
cargo build --all-features
```

**Release Build (Optimized):**
```bash
# Build with optimizations
cargo build --release

# Binary will be in target/release/
```

**Check (Compile without linking):**
```bash
# Faster than full build, checks for errors
cargo check

# Check with all features
cargo check --all-features
```

### Build Profiles

**Available profiles** (in `Cargo.toml`):
```toml
[profile.dev]
opt-level = 0        # No optimizations (fast compilation)
debug = true         # Include debug info

[profile.release]
opt-level = 3        # Full optimizations
debug = false        # No debug info
lto = true          # Link-time optimization

[profile.bench]
opt-level = 3
debug = false
```

**Using profiles:**
```bash
cargo build --profile release
cargo build --profile bench
```

### Incremental Compilation

Speed up rebuilds:
```bash
# Enable incremental compilation (default in dev)
export CARGO_INCREMENTAL=1

# Disable for clean builds
export CARGO_INCREMENTAL=0
```

### Build Times

**First build**: ~2-5 minutes (downloads and compiles dependencies)  
**Incremental builds**: ~5-30 seconds (only changed crates)  
**Clean build**: ~1-3 minutes (no dependency download)

**Tips for faster builds:**
```bash
# Use cargo-watch for automatic rebuilds
cargo watch -x build

# Use sccache for caching compilations
cargo install sccache
export RUSTC_WRAPPER=sccache

# Parallel compilation (default based on CPU cores)
export CARGO_BUILD_JOBS=8
```

---

## Running Tests

### Test Structure

```
forgedb/
├── crates/
│   └── storage/
│       ├── src/
│       │   └── lib.rs        # Unit tests here
│       └── tests/
│           └── integration_test.rs
└── tests/                    # Workspace integration tests
```

### Running Tests

**All Tests:**
```bash
# Run all tests in workspace
cargo test --all

# Run with output
cargo test --all -- --nocapture

# Run with specific test threads
cargo test -- --test-threads=1
```

**Unit Tests Only:**
```bash
# Run unit tests (in src/)
cargo test --lib

# Specific crate
cargo test --lib --package forgedb-storage
```

**Integration Tests Only:**
```bash
# Run integration tests (in tests/)
cargo test --test '*'

# Specific integration test
cargo test --test storage_integration
```

**Documentation Tests:**
```bash
# Run doc tests (examples in /// comments)
cargo test --doc

# Specific crate
cargo test --doc --package forgedb-parser
```

**Specific Test:**
```bash
# Run test by name
cargo test test_parse_model

# Run tests matching pattern
cargo test parse

# Run in specific package
cargo test test_parse_model --package forgedb-parser
```

### Test Options

**Show Output:**
```bash
# Show println! output from passing tests
cargo test -- --nocapture

# Show output from failing tests only (default)
cargo test
```

**Run Ignored Tests:**
```bash
# Run tests marked with #[ignore]
cargo test -- --ignored

# Run all tests including ignored
cargo test -- --include-ignored
```

**Test Coverage:**
```bash
# Generate coverage report (requires cargo-tarpaulin)
cargo tarpaulin --out Html --output-dir coverage/

# View report
open coverage/index.html
```

### Test Performance

**Benchmark Tests:**
```bash
# Run benchmark tests
cargo bench

# Specific benchmark
cargo bench --bench storage_benchmarks
```

**Test Timing:**
```bash
# Show test execution time
cargo test -- --nocapture --test-threads=1
```

---

## Local Testing

### Testing CLI Commands

**Build CLI:**
```bash
cargo build --bin forgedb
```

**Test Commands:**
```bash
# Test init command
./target/debug/forgedb init test-project
cd test-project
ls -la

# Test validate command
./target/debug/forgedb validate schema.forge

# Test build command
./target/debug/forgedb build

# Test dev command (in background)
./target/debug/forgedb dev &
curl http://localhost:3000/health
```

### Testing Code Generation

**Create Test Schema:**
```bash
mkdir -p /tmp/test-forgedb
cd /tmp/test-forgedb

cat > schema.forge << 'EOF'
User {
  id: +uuid
  email: ^&string
  username: ^&string
  created_at: ^timestamp
  
  @index(email)
  @pattern(email, "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$")
}

Post {
  id: +uuid
  title: ^string
  content: string
  author: *User
  created_at: ^timestamp
  
  @index(author, created_at)
}
EOF
```

**Generate Code:**
```bash
# Use your dev version of CLI
/path/to/forgedb/target/debug/forgedb build

# Check generated files
ls -la generated/
cat generated/user.rs
cat generated/post.rs
cat generated/lib.rs
```

**Test Generated Code:**
```bash
# Create minimal Cargo.toml
cat > Cargo.toml << 'EOF'
[package]
name = "test-forgedb"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = { path = "/path/to/forgedb/crates/storage" }
forgedb-crud-api = { path = "/path/to/forgedb/crates/crud-api" }
uuid = { version = "1.0", features = ["v4", "serde"] }
serde = { version = "1.0", features = ["derive"] }
EOF

# Try to build
cargo build
```

### Testing HTTP Server

**Start Test Server:**
```bash
# Build and run example
cargo run -- generate all --output ./generated

# Or use CLI
cd /tmp/test-forgedb
/path/to/forgedb/target/debug/forgedb dev
```

**Test Endpoints:**
```bash
# Health check
curl http://localhost:3000/health

# Create user
curl -X POST http://localhost:3000/users \
  -H "Content-Type: application/json" \
  -d '{"email": "test@example.com", "username": "testuser"}'

# Get user
curl http://localhost:3000/users/{id}

# List users
curl http://localhost:3000/users?page=1&limit=10

# Metrics
curl http://localhost:3000/metrics
```

### Testing Storage Engine

**Interactive Test:**
```rust
// Create tests/manual_test.rs
use forgedb_storage::*;
use std::path::PathBuf;

#[test]
fn manual_storage_test() {
    let db_path = PathBuf::from("/tmp/test-db");
    
    // Clean up
    let _ = std::fs::remove_dir_all(&db_path);
    
    // Create database
    let mut db = Database::open(db_path.clone()).unwrap();
    
    // Define schema
    db.set_columns(vec![
        ColumnMetadata {
            name: "id".to_string(),
            column_type: ColumnType::U64,
            column_index: 0,
        },
        ColumnMetadata {
            name: "email".to_string(),
            column_type: ColumnType::String,
            column_index: 0,
        },
    ]);
    db.save_manifest().unwrap();
    
    // Open columns
    let mut id_col = FixedColumn::new(db.fixed_column_path(0), 8).unwrap();
    let mut email_col = VariableColumn::new(
        db.variable_data_path(0),
        db.variable_offsets_path(0)
    ).unwrap();
    
    // Insert data
    id_col.append_u64(1).unwrap();
    email_col.append_string("test@example.com").unwrap();
    
    // Read data
    assert_eq!(id_col.read_u64(0).unwrap(), 1);
    assert_eq!(email_col.read_string(0).unwrap(), "test@example.com");
    
    println!("✓ Manual storage test passed");
}
```

**Run test:**
```bash
cargo test manual_storage_test -- --nocapture
```

---

## Debugging Tips

### Enable Debug Logging

**Set log level:**
```bash
# All debug logs
RUST_LOG=debug cargo run

# Specific module
RUST_LOG=forgedb_parser=debug cargo run

# Multiple modules
RUST_LOG=forgedb_parser=debug,forgedb_storage=trace cargo run
```

**In code:**
```rust
use tracing::{debug, info, warn, error};

fn parse_schema(input: &str) -> Result<Schema> {
    debug!("Parsing schema of length {}", input.len());
    
    let tokens = tokenize(input)?;
    debug!("Tokenized into {} tokens", tokens.len());
    
    let ast = parse_tokens(tokens)?;
    info!("Successfully parsed schema with {} models", ast.models.len());
    
    Ok(ast)
}
```

### Using Rust Debugger

**With GDB (Linux):**
```bash
# Build with debug info
cargo build

# Run with GDB
rust-gdb target/debug/forgedb
(gdb) break main
(gdb) run
(gdb) next
(gdb) print variable_name
```

**With LLDB (macOS):**
```bash
rust-lldb target/debug/forgedb
(lldb) breakpoint set --name main
(lldb) run
(lldb) step
(lldb) print variable_name
```

**With VSCode:**
```json
// .vscode/launch.json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug ForgeDB",
      "cargo": {
        "args": ["build", "--bin=forgedb"]
      },
      "args": ["dev"],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

### Inspecting Data Structures

**Pretty-print Debug:**
```rust
let schema = parse_schema(input)?;
println!("{:#?}", schema);  // Pretty print
```

**Using dbg! macro:**
```rust
let result = some_function(arg);
dbg!(&result);  // Prints with file/line info
```

**Inspecting AST:**
```rust
let schema = parse_schema(input)?;
for model in &schema.models {
    println!("Model: {}", model.name);
    for field in &model.fields {
        println!("  Field: {} : {:?}", field.name, field.field_type);
    }
}
```

### Performance Profiling

**CPU Profiling with Flamegraph:**
```bash
# Install
cargo install flamegraph

# Profile program
cargo flamegraph --bin forgedb

# Profile test
cargo flamegraph --test storage_test

# Open flamegraph.svg in browser
```

**Memory Profiling:**
```bash
# Use valgrind (Linux)
cargo build
valgrind --leak-check=full ./target/debug/forgedb

# Use heaptrack (Linux)
heaptrack ./target/debug/forgedb
```

**Benchmarking:**
```bash
# Run benchmarks
cargo bench

# Compare benchmarks
cargo bench -- --save-baseline before
# Make changes
cargo bench -- --baseline before
```

### Common Issues

**Issue: Compilation errors after pulling changes**
```bash
# Clean and rebuild
cargo clean
cargo build
```

**Issue: Test failures**
```bash
# Run specific test with output
cargo test failing_test_name -- --nocapture

# Check if tests pass individually
cargo test --lib -- --test-threads=1
```

**Issue: Slow compile times**
```bash
# Use cargo-watch for incremental builds
cargo watch -x check

# Use sccache
export RUSTC_WRAPPER=sccache
cargo build
```

**Issue: Out of memory during compilation**
```bash
# Reduce parallel compilation
export CARGO_BUILD_JOBS=2
cargo build
```

---

## IDE Configuration

### VSCode

**Extensions:**
```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "vadimcn.vscode-lldb",
    "serayuzgur.crates",
    "tamasfe.even-better-toml"
  ]
}
```

**Settings** (`.vscode/settings.json`):
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.checkOnSave.allTargets": true,
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.tabSize": 4
  },
  "rust-analyzer.lens.enable": true,
  "rust-analyzer.inlayHints.enable": true
}
```

**Tasks** (`.vscode/tasks.json`):
```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "type": "cargo",
      "command": "build",
      "problemMatcher": ["$rustc"],
      "group": "build",
      "label": "rust: cargo build"
    },
    {
      "type": "cargo",
      "command": "test",
      "problemMatcher": ["$rustc"],
      "group": "test",
      "label": "rust: cargo test"
    }
  ]
}
```

### IntelliJ IDEA / CLion

**Install Rust Plugin:**
1. Go to Settings > Plugins
2. Search for "Rust"
3. Install and restart

**Configuration:**
- Enable Cargo check on save
- Enable Rustfmt on save
- Configure external tools for Clippy

### Vim/Neovim

**rust-analyzer with coc.nvim:**
```vim
" Install coc.nvim
Plug 'neoclide/coc.nvim', {'branch': 'release'}

" Install rust-analyzer
:CocInstall coc-rust-analyzer

" Configuration
{
  "rust-analyzer.server.path": "rust-analyzer",
  "rust-analyzer.cargo.features": "all"
}
```

### Emacs

**rust-mode with lsp-mode:**
```elisp
(use-package rust-mode
  :hook (rust-mode . lsp))

(use-package lsp-mode
  :commands lsp
  :config
  (setq lsp-rust-analyzer-cargo-watch-command "clippy"))
```

---

## Common Development Tasks

### Adding a New Crate

```bash
# Create new crate in workspace
cargo new --lib crates/my-new-crate

# Add to workspace Cargo.toml
# [workspace]
# members = [
#   "crates/my-new-crate",
# ]

# Add dependencies
cd crates/my-new-crate
cargo add serde --features derive
```

### Updating Dependencies

```bash
# Check for outdated dependencies
cargo outdated

# Update dependencies
cargo update

# Update specific dependency
cargo update -p serde

# Upgrade to latest (using cargo-edit)
cargo upgrade
```

### Running Clippy

```bash
# Run clippy on all targets
cargo clippy --all-targets --all-features

# Fix automatically (when possible)
cargo clippy --fix

# Deny warnings
cargo clippy -- -D warnings
```

### Formatting Code

```bash
# Format all code
cargo fmt --all

# Check formatting without changing
cargo fmt --all -- --check

# Format specific file
rustfmt src/main.rs
```

### Generating Documentation

```bash
# Build docs for all crates
cargo doc --no-deps

# Build and open in browser
cargo doc --no-deps --open

# Include private items
cargo doc --document-private-items
```

### Cleaning Build Artifacts

```bash
# Remove target directory
cargo clean

# Remove specific package artifacts
cargo clean --package forgedb-storage

# Remove only old artifacts (keep recent)
cargo sweep --time 30  # Keep last 30 days
```

---

## Next Steps

After setting up your development environment:

1. Read [CONTRIBUTING.md](./CONTRIBUTING.md) for contribution guidelines
2. Check [GitHub Issues](https://github.com/yourusername/forgedb/issues) for tasks
3. Look for issues labeled `good-first-issue`
4. Join our community (Discord, etc.)

---

## Additional Resources

- [Architecture Documentation](./ARCHITECTURE.md)
- [Public Crates Guide](./PUBLIC_CRATES.md)
- [Internal Crates Guide](./INTERNAL_CRATES.md)
- [Contributing Guide](./CONTRIBUTING.md)
- [Publishing Guide](./PUBLISHING.md)

---

**Need Help?** Open an issue on GitHub or ask in our community channels.
