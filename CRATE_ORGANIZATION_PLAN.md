# ForgeDB Crate Organization & Documentation Plan

**Created:** 2025-10-20
**Status:** In Progress
**Total Issues:** 18 (GitHub #9-#26)

---

## Executive Summary

This plan addresses the organization, documentation, and preparation of ForgeDB's 15 crates for eventual separation into public (runtime) and internal (tooling) repositories. The audit revealed critical gaps including an empty `forgedb-types` crate, missing documentation across most crates, and inconsistent metadata.

### Key Findings

- **15 Total Crates**: 10 public (runtime), 5 internal (tooling)
- **Critical Issue**: `forgedb-types` is completely empty
- **Documentation Gap**: Only 3/15 crates have README files
- **Metadata Gap**: 7/10 public crates missing Cargo.toml metadata
- **Testing Gap**: 2 public crates have no tests
- **Examples Gap**: 0/15 crates have examples directories

### Success Metrics

After completion:
- ✅ All 15 crates fully documented with READMEs
- ✅ All public crates have metadata, tests, and examples
- ✅ Clear public/internal separation ready for repo split
- ✅ Unified documentation structure in docs/
- ✅ All crates can build and publish independently

---

## Crate Classification

### Public Crates (Runtime Libraries)

These crates provide functionality that generated code uses. They will be moved to a public repository.

| Crate | Purpose | Status | Dependencies |
|-------|---------|--------|--------------|
| **forgedb-types** | Core type definitions | ⚠️ **EMPTY** | None |
| **forgedb-storage** | Columnar storage engine | ✅ Good | wal, serde, uuid |
| **forgedb-wal** | Write-Ahead Log | ✅ Good | serde, uuid, crc32fast |
| **forgedb-http-server** | HTTP/REST server | ⚠️ Needs docs | axum, tower-http |
| **forgedb-crud-api** | CRUD operations | ⚠️ Needs docs+tests | storage, serde, uuid |
| **forgedb-query-params** | Query param parsing | ✅ Good | serde |
| **forgedb-compaction** | DB compaction | ⚠️ Needs docs | serde, chrono |
| **forgedb-fulltext** | Full-text search | ⚠️ Needs docs+tests | uuid, regex |
| **forgedb-query-optimization** | Query optimization | ✅ Has README | storage, types |
| **forgedb-ffi** | Foreign Function Interface | ⚠️ Needs docs | storage, types, libc |

**Total**: 10 public crates

### Internal Crates (Tooling & CLI)

These crates handle code generation, CLI, and development tools. They remain in the private repository.

| Crate | Purpose | Status | Dependencies |
|-------|---------|--------|--------------|
| **forgedb-parser** | Schema parser | ⚠️ Needs README | validation |
| **forgedb-validation** | Schema validation | ⚠️ Needs all docs | None |
| **forgedb-watcher** | File watching | ✅ Has README | notify, parser |
| **forgedb-migrations** | Schema migrations | ⚠️ Needs README | serde, chrono, uuid |
| **forgedb-lsp-server** | LSP for IDE support | ✅ Has README | tower-lsp, tokio |

**Total**: 5 internal crates (+ 1 CLI binary)

---

## Current State Assessment

### Per-Crate Analysis

#### Public Crates

##### forgedb-types
**Status**: 🔴 **CRITICAL - Empty Crate**
- ❌ Completely empty (only a comment)
- ❌ No implementation
- ❌ No metadata
- ❌ No README
- ❌ No tests
- **Action**: Issue #9 - Complete implementation required

##### forgedb-storage
**Status**: 🟡 Partially Complete
- ✅ Has metadata (license, description, version)
- ✅ Good crate-level architecture comments
- ✅ WAL integration
- ❌ No README
- ❌ No examples
- ⚠️ Minimal crate docs
- **Action**: Issue #11 - Add README

##### forgedb-wal
**Status**: 🟢 Good
- ✅ Has metadata
- ✅ Excellent crate-level docs
- ✅ Has tests (3 test files)
- ❌ No README
- ❌ No examples
- **Action**: Issue #11 - Add README, examples

##### forgedb-http-server
**Status**: 🟡 Partially Complete
- ❌ No metadata
- ✅ Extensive tests (8 test files)
- ✅ Rich feature set (TLS, auth, caching, metrics)
- ❌ No README
- ❌ No examples
- **Action**: Issue #10, #12 - Add metadata + README

##### forgedb-crud-api
**Status**: 🔴 Needs Work
- ❌ No metadata
- ❌ No README
- ❌ No tests
- ❌ No examples
- ✅ Clear public API
- **Action**: Issue #10, #13, #19 - All documentation + tests

##### forgedb-query-params
**Status**: 🟢 Good
- ❌ No metadata
- ✅ Has crate-level docs
- ✅ Has tests (4 test files)
- ❌ No README
- ❌ No examples
- **Action**: Issue #10, #14 - Add metadata + README

##### forgedb-compaction
**Status**: 🟡 Partially Complete
- ❌ No metadata
- ✅ Has tests
- ✅ Background worker implementation
- ❌ No README
- ❌ No examples
- **Action**: Issue #10, #15 - Add metadata + README

##### forgedb-fulltext
**Status**: 🔴 Needs Work
- ❌ No metadata
- ✅ Has crate-level docs
- ❌ No README
- ❌ No examples
- ❌ No tests
- **Action**: Issue #10, #16, #19 - All documentation + tests

##### forgedb-query-optimization
**Status**: 🟢 Best in Class
- ❌ No metadata
- ✅ **Has README** (comprehensive)
- ✅ Has benchmarks
- ✅ Well-documented SIMD optimizations
- ❌ No examples
- **Action**: Issue #10 - Add metadata + examples

##### forgedb-ffi
**Status**: 🟡 Partially Complete
- ❌ No metadata
- ✅ Has tests (3 test files)
- ✅ cbindgen setup
- ❌ No README
- ❌ No examples
- **Action**: Issue #10, #17 - Add metadata + README

#### Internal Crates

##### forgedb-parser
**Status**: 🟡 Partially Complete
- ✅ Has metadata
- ❌ No crate-level docs
- ❌ No README
- ❌ No examples
- ✅ Clean AST structure
- **Action**: Issue #21 - Add README + docs

##### forgedb-validation
**Status**: 🟡 Partially Complete
- ❌ No metadata
- ✅ Has tests (3 test files)
- ❌ No README
- ✅ HTTP validation support
- **Action**: Issue #20, #22 - Add metadata + README

##### forgedb-watcher
**Status**: 🟢 Good
- ❌ No metadata
- ✅ **Has README** (comprehensive)
- ✅ Has tests
- ✅ Good documentation
- ❌ No examples
- **Action**: Issue #20 - Add metadata

##### forgedb-migrations
**Status**: 🟡 Partially Complete
- ❌ No metadata
- ✅ Has tests (3 test files)
- ❌ No README
- ✅ Diff/execute/track implementation
- **Action**: Issue #20, #23 - Add metadata + README

##### forgedb-lsp-server
**Status**: 🟢 Good
- ❌ No metadata
- ✅ **Has README** (detailed)
- ❌ No tests
- ✅ Rich LSP features
- **Action**: Issue #20 - Add metadata

---

## Five-Phase Implementation Plan

### Phase 1: Public Crate Completeness (Critical Priority)

**Goal**: Ensure all public crates are production-ready with complete metadata and documentation.

**Duration**: 2-3 weeks

#### Issues

1. **#9 - [CRITICAL] Implement forgedb-types crate**
   - Implement all core types (i32, i64, f64, bool, uuid, timestamp, string)
   - Add comprehensive tests
   - Add metadata
   - Write README
   - Create examples
   - **Blocker**: This is foundational

2. **#10 - Add metadata to all public crates**
   - forgedb-http-server
   - forgedb-crud-api
   - forgedb-query-params
   - forgedb-compaction
   - forgedb-fulltext
   - forgedb-query-optimization
   - forgedb-ffi
   - **Required**: license, description, repository, keywords, categories

3. **#11 - Write README for forgedb-storage**
   - Architecture (columnar format, mmap)
   - Usage examples
   - API documentation
   - Performance characteristics
   - File layout

4. **#12 - Write README for forgedb-http-server**
   - Server configuration
   - TLS setup
   - Middleware (auth, rate limiting, caching)
   - Health checks and metrics

5. **#13 - Write README for forgedb-crud-api**
   - CRUD operations
   - Usage examples
   - Error handling
   - Storage integration

6. **#14 - Write README for forgedb-query-params**
   - Filter, sort, pagination APIs
   - Parsing examples
   - Supported operators
   - REST integration

7. **#15 - Write README for forgedb-compaction**
   - Compaction strategies
   - Configuration
   - Background compaction
   - Performance implications

8. **#16 - Write README for forgedb-fulltext**
   - Indexing and search
   - Tokenization
   - TF-IDF scoring
   - Query syntax

9. **#17 - Write README for forgedb-ffi**
   - C API
   - FFI usage (C/Python/Node)
   - Memory management
   - cbindgen setup

**Deliverables**:
- ✅ forgedb-types fully implemented
- ✅ All public crates have Cargo.toml metadata
- ✅ All public crates have comprehensive READMEs

---

### Phase 2: Examples & Tests

**Goal**: Add practical examples and comprehensive tests to all public crates.

**Duration**: 1-2 weeks

#### Issues

10. **#18 - Add examples to all public crates**
    - Create examples/ directory for each crate
    - 2-3 runnable examples per crate:
      - Basic example (hello world)
      - Intermediate example (realistic use case)
      - Advanced example (complex scenario, optional)
    - All examples must be runnable with `cargo run --example <name>`

11. **#19 - Add tests to undertested public crates**
    - **forgedb-crud-api**: Add CRUD, error, integration tests
    - **forgedb-fulltext**: Add indexing, search, tokenization, scoring tests
    - **Goal**: >80% coverage on public APIs

**Deliverables**:
- ✅ All public crates have examples/ directory
- ✅ At least 2 examples per crate
- ✅ forgedb-crud-api and forgedb-fulltext fully tested

---

### Phase 3: Internal Crate Documentation

**Goal**: Document internal crates for maintainability and onboarding.

**Duration**: 1 week

#### Issues

12. **#20 - Add metadata to all internal crates**
    - forgedb-parser (verify/complete)
    - forgedb-validation
    - forgedb-watcher
    - forgedb-migrations
    - forgedb-lsp-server

13. **#21 - Write README for forgedb-parser**
    - Schema language syntax
    - Parsing examples
    - AST structure
    - Error handling

14. **#22 - Write README for forgedb-validation**
    - Schema validation rules
    - HTTP validation
    - Status code mapping
    - Error reporting

15. **#23 - Write README for forgedb-migrations**
    - Schema diffing algorithm
    - Migration generation
    - Migration execution
    - Tracking mechanism

**Deliverables**:
- ✅ All internal crates have Cargo.toml metadata
- ✅ All internal crates have READMEs
- ✅ Internal architecture documented

---

### Phase 4: Crate-Level Documentation

**Goal**: Add comprehensive crate-level documentation to all lib.rs files.

**Duration**: 3-4 days

#### Issues

16. **#24 - Add comprehensive crate-level docs to all lib.rs files**
    - Add //! module documentation to all 15 crates
    - Include:
      - Crate overview
      - Architecture (where applicable)
      - Usage examples
      - Public API overview
      - Links to external docs

**Format**:
```rust
//! ForgeDB Storage Engine
//!
//! This crate provides a columnar storage engine for ForgeDB with:
//! - Memory-mapped file access for performance
//! - Fixed and variable-length column support
//! - WAL integration for durability
//! - Tombstone tracking for deletions
//!
//! # Architecture
//!
//! The storage engine uses a columnar format...
//!
//! # Examples
//!
//! ```
//! use forgedb_storage::Database;
//! let db = Database::new("./data")?;
//! ```
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [`forgedb-wal`](../wal) for durability
```

**Deliverables**:
- ✅ All 15 lib.rs files have comprehensive //! docs
- ✅ Documentation includes examples
- ✅ Architecture explained where relevant

---

### Phase 5: Repository Organization

**Goal**: Prepare for public/private repo split and create unified documentation.

**Duration**: 1 week

#### Issues

17. **#25 - Prepare public crate extraction**
    - Create dependency graph (verify no internal→public deps)
    - Set up CI/CD for public repo
    - Document extraction process
    - Align versions across public crates
    - Test independent builds

18. **#26 - Create unified documentation**
    - Create docs/ directory with:
      - ARCHITECTURE.md (overall system design)
      - PUBLIC_CRATES.md (runtime library guide)
      - INTERNAL_CRATES.md (tooling guide)
      - CONTRIBUTING.md (contribution guidelines)
      - DEVELOPMENT.md (development setup)
      - PUBLISHING.md (release process)
    - Update all crate READMEs to link to central docs

**Deliverables**:
- ✅ Dependency graph shows clean separation
- ✅ Public crates can build independently
- ✅ CI/CD configured
- ✅ Unified documentation in docs/

---

## Dependency Graph

### Public Crates

```
forgedb-types (foundational, no deps)
    ↑
    ├── forgedb-storage
    │       ↑
    │       ├── forgedb-wal
    │       ├── forgedb-crud-api
    │       └── forgedb-query-optimization
    │
    └── forgedb-ffi

forgedb-http-server (independent)
forgedb-query-params (independent)
forgedb-compaction (independent)
forgedb-fulltext (independent)
```

**Key Insight**: No public crate depends on internal crates ✅

### Internal Crates

```
forgedb-validation (foundational)
    ↑
    └── forgedb-parser
            ↑
            ├── forgedb-watcher
            └── forgedb-migrations

forgedb-lsp-server (independent)
```

**Key Insight**: Clean separation from public crates ✅

---

## Implementation Strategy

### Parallel Work Streams

1. **Critical Path** (blocking):
   - Issue #9 (forgedb-types implementation) → Must complete first

2. **Public Documentation** (can run in parallel after #9):
   - Issues #10-17 (metadata + READMEs for public crates)

3. **Testing & Examples** (can run in parallel):
   - Issue #18 (examples)
   - Issue #19 (tests)

4. **Internal Documentation** (low priority):
   - Issues #20-23 (internal crate metadata + READMEs)

5. **Final Polish** (sequential):
   - Issue #24 (lib.rs docs) → requires READMEs done
   - Issue #25 (extraction prep) → requires all public crates complete
   - Issue #26 (unified docs) → requires everything done

### Recommended Order

**Week 1**: Issues #9, #10
**Week 2**: Issues #11-17 (public READMEs)
**Week 3**: Issues #18-19 (examples + tests)
**Week 4**: Issues #20-23 (internal docs)
**Week 5**: Issues #24-26 (final polish)

---

## Success Criteria

### Per-Crate Checklist

Each crate must have:
- [ ] Complete Cargo.toml metadata (license, description, repository, keywords, categories)
- [ ] Comprehensive README.md
- [ ] Crate-level docs in lib.rs (//!)
- [ ] Public crates: examples/ directory with 2+ examples
- [ ] Public crates: >80% test coverage on public APIs
- [ ] All tests passing

### Overall Goals

- [ ] All 18 issues completed and closed
- [ ] All crates can build independently
- [ ] Public crates ready for crates.io publication
- [ ] Documentation is comprehensive and accurate
- [ ] Clean public/internal separation verified
- [ ] CI/CD configured for both repos
- [ ] Unified docs/ structure created

---

## Issue Tracking

### GitHub Issues Created

| Phase | Issue # | Title | Status |
|-------|---------|-------|--------|
| 1 | #9 | [CRITICAL] Implement forgedb-types crate | Open |
| 1 | #10 | Add metadata to all public crates | Open |
| 1 | #11 | Write README for forgedb-storage | Open |
| 1 | #12 | Write README for forgedb-http-server | Open |
| 1 | #13 | Write README for forgedb-crud-api | Open |
| 1 | #14 | Write README for forgedb-query-params | Open |
| 1 | #15 | Write README for forgedb-compaction | Open |
| 1 | #16 | Write README for forgedb-fulltext | Open |
| 1 | #17 | Write README for forgedb-ffi | Open |
| 2 | #18 | Add examples to all public crates | Open |
| 2 | #19 | Add tests to undertested public crates | Open |
| 3 | #20 | Add metadata to all internal crates | Open |
| 3 | #21 | Write README for forgedb-parser | Open |
| 3 | #22 | Write README for forgedb-validation | Open |
| 3 | #23 | Write README for forgedb-migrations | Open |
| 4 | #24 | Add comprehensive crate-level docs to all lib.rs | Open |
| 5 | #25 | Prepare public crate extraction | Open |
| 5 | #26 | Create unified documentation | Open |

**Total**: 18 issues across 5 phases

View all issues: https://github.com/hoodiecollin/forgedb/issues?q=is%3Aissue+is%3Aopen+label%3Adocumentation,enhancement

---

## Future Considerations

### Post-Split Repository Structure

**Public Repo** (`forgedb-runtime`):
```
forgedb-runtime/
├── crates/
│   ├── types/
│   ├── storage/
│   ├── wal/
│   ├── http-server/
│   ├── crud-api/
│   ├── query-params/
│   ├── compaction/
│   ├── fulltext/
│   ├── query-optimization/
│   └── ffi/
├── docs/
├── examples/
├── Cargo.toml (workspace)
└── README.md
```

**Private Repo** (`forgedb`):
```
forgedb/
├── crates/
│   ├── parser/
│   ├── validation/
│   ├── watcher/
│   ├── migrations/
│   └── lsp-server/
├── src/ (CLI)
├── docs/
├── vscode-forgedb/
└── README.md
```

### Version Strategy

- Public crates: Synchronized versions (e.g., all at v0.2.0)
- Internal crates: Independent versions (tooling can evolve separately)
- CLI: Independent version (tracks feature set, not library version)

---

## References

- **Sprint Plan**: [SPRINT_PLAN_COMPLETE.md](./archive/SPRINT_PLAN_COMPLETE.md)
- **GitHub Issues**: https://github.com/hoodiecollin/forgedb/issues
- **Crate Guidelines**: https://doc.rust-lang.org/cargo/reference/manifest.html
- **Publishing Guide**: https://doc.rust-lang.org/cargo/reference/publishing.html

---

## Appendix: Metadata Template

### Public Crate Cargo.toml

```toml
[package]
name = "forgedb-<name>"
version = "0.2.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "<concise description>"
repository = "https://github.com/hoodiecollin/forgedb"
keywords = ["database", "forgedb", "<relevant>", "<keywords>"]
categories = ["database", "<relevant-category>"]
readme = "README.md"

[dependencies]
# ...
```

### Internal Crate Cargo.toml

```toml
[package]
name = "forgedb-<name>"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "<concise description>"
repository = "https://github.com/hoodiecollin/forgedb"
keywords = ["forgedb", "tooling", "<relevant>"]
categories = ["development-tools"]

[dependencies]
# ...
```

---

**Plan created**: 2025-10-20
**Last updated**: 2025-10-20
**Status**: Ready for execution
