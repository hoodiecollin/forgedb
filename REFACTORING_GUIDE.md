# Unit Test Refactoring Guide

This document describes the pattern for moving unit tests from source files to dedicated test files in the ForgeDB project.

## Status

### Completed ✅
- **Main src/ modules** (7 files):
  - `src/lexer.rs` → `tests/lexer_tests.rs`
  - `src/ast.rs` → `tests/ast_tests.rs`
  - `src/api_codegen.rs` → `tests/api_codegen_tests.rs`
  - `src/openapi_codegen.rs` → `tests/openapi_codegen_tests.rs`
  - `src/typescript_codegen.rs` → `tests/typescript_codegen_tests.rs`
  - `src/typescript_component_props.rs` → `tests/typescript_component_props_tests.rs`
  - `src/parser/mod.rs` → Already separated in `src/parser/tests.rs`

### Remaining 📋
**Crate modules** (47 files across 14 crates):
  - cli: 2 files
  - compaction: 4 files
  - crud-api: 3 files
  - ffi: 4 files
  - fulltext: 1 file
  - http-server: 8 files
  - lsp-server: 1 file
  - migrations: 4 files
  - query-optimization: 3 files
  - query-params: 3 files
  - storage: 2 files
  - validation: 3 files
  - wal: 5 files
  - watcher: 2 files

## Refactoring Pattern

For each source file with embedded unit tests (`#[cfg(test)] mod tests { ... }`):

### Step 1: Create Test File

For a crate at `crates/CRATE_NAME/src/MODULE.rs`:
1. Create `crates/CRATE_NAME/tests/` directory if it doesn't exist
2. Create `crates/CRATE_NAME/tests/MODULE_tests.rs`

### Step 2: Extract and Adapt Tests

```rust
// Original in src/MODULE.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_something() {
        // test code
    }
}

// New in tests/MODULE_tests.rs
use CRATE_NAME::*;  // Or specific imports

#[test]
fn test_something() {
    // test code (same)
}
```

### Step 3: Update Visibility

Tests may need access to private functions/types. Make them `pub`:

```rust
// Before
fn internal_helper() { }

// After
pub fn internal_helper() { }  // Or pub(crate) if appropriate
```

### Step 4: Remove Test Module

Remove the entire `#[cfg(test)] mod tests { ... }` block from the source file.

### Step 5: Verify

```bash
# Test the specific crate
cargo test -p CRATE_NAME

# Or test everything
cargo test
```

## Example: Storage Crate

### Before: `crates/storage/src/lib.rs`
```rust
// ... implementation code ...

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_fixed_column_u64() {
        let temp_dir = std::env::temp_dir().join("forgedb_test_fixed");
        // ... test implementation ...
    }
}
```

### After: `crates/storage/tests/lib_tests.rs`
```rust
use forgedb_storage::*;
use std::fs;

#[test]
fn test_fixed_column_u64() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_fixed");
    // ... test implementation (unchanged) ...
}
```

## Common Patterns

### Import Adjustments
- `use super::*;` → `use CRATE_NAME::*;` or specific imports
- Helper functions may need to be made public or duplicated in tests

### Test Organization
- One test file per source module maintains clear correspondence
- Integration tests can remain in `tests/` directory
- Helper functions for tests can go in `tests/common/mod.rs`

### Visibility Changes
When tests are external, you may need to:
- Make structs/enums public: `pub struct MyStruct { ... }`
- Make methods public: `pub fn my_method() { ... }`
- Use `pub(crate)` for internal-only visibility
- Document with `#[doc(hidden)]` if needed for public API but not documented

## Benefits

1. **Cleaner Source Files**: Application logic separated from test code
2. **Better Organization**: Tests grouped in dedicated directories
3. **Compilation Speed**: Tests only compiled when needed
4. **Independence**: Integration tests run in separate compilation units

## Testing Commands

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p forgedb-storage

# Run specific test file (integration test)
cargo test --test lib_tests

# Run library unit tests only
cargo test --lib
```

## Automation Potential

For bulk refactoring, consider:
1. Extract test blocks: `awk '/#\[cfg\(test\)\]/{flag=1} flag{print}' src/file.rs`
2. Remove test blocks: `head -n $(grep -n "#\[cfg(test)\]" src/file.rs | cut -d: -f1 | head -1) src/file.rs`
3. Adjust imports programmatically
4. Run tests after each refactoring to verify

## Notes

- Parser tests in `src/parser/tests.rs` are already separated - keep this structure
- Some crates may have existing `tests/` directories - add new test files there
- Ensure test names remain descriptive and unique within the test suite
- Test helper functions should be moved to `tests/common.rs` if shared
