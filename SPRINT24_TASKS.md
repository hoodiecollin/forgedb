# Sprint 24: Bun FFI Runtime - Detailed Task Breakdown

**Branch**: `sprint-24/bun-ffi-runtime`
**Status**: Not Started
**Created**: 2025-10-15
**Dependencies**: Sprint 17 (UI Component Integration) ✅ COMPLETE

---

## Executive Summary

**Goal**: Enable direct ForgeDB database access from Bun runtime via FFI, eliminating HTTP overhead for read operations in component rendering and route handlers.

**Current Architecture (Sprint 17)**:
```
Bun Server (Port 3001) → HTTP → Rust API (Port 3000) → ForgeDB
Latency: ~1-10ms per request
```

**Target Architecture (Sprint 24)**:
```
Bun Server → FFI (C ABI) → ForgeDB (read-only)
Latency: ~50-100μs per request (10-100x improvement)
```

**Scope**:
- Read-only operations: get, list, query, relation traversal
- Write operations remain in Rust API server (maintained separation)
- Thread-safe concurrent access
- Zero-copy data access where possible
- Automatic memory management

---

## Technical Decisions & Rationale

### Decision 1: Read-Only FFI Access
**Chosen**: Read operations only via FFI
**Rationale**:
- Maintains clear separation: writes go through validated Rust API
- Reduces FFI complexity (no transaction management)
- Component rendering is read-heavy (95%+ reads)
- Simplifies memory management and safety

### Decision 2: Handle-Based Memory Management
**Chosen**: Opaque handle pointers with registry
**Rationale**:
- Safe: handles validated before use
- Predictable: explicit lifecycle
- Compatible: standard C FFI pattern
- Clean: automatic cleanup with FinalizationRegistry

**Example**:
```c
ForgeDB* db = forgedb_open("./data", READONLY);
// handle is just an ID, actual data in Rust registry
forgedb_close(db);  // removes from registry
```

### Decision 3: JSON String Returns
**Chosen**: All data returned as JSON strings
**Rationale**:
- Simple: no complex struct marshalling
- Safe: clear ownership (caller frees)
- Flexible: works with any data structure
- TypeScript-friendly: direct JSON.parse()

**Trade-off**: Some serialization overhead, but still 10x faster than HTTP

### Decision 4: Error Handling via Error Pointers
**Chosen**: Out-parameter error pointers + return NULL
**Rationale**:
- Standard C FFI pattern
- Allows rich error messages
- Compatible with Bun FFI
- Clear success/failure indication

**Example**:
```c
ForgeDBError* err = NULL;
ForgeDB* db = forgedb_open("./data", READONLY, &err);
if (db == NULL) {
    printf("Error: %s\n", forgedb_error_message(err));
    forgedb_free_error(err);
}
```

### Decision 5: Thread-Safe Shared Handle
**Chosen**: Single database handle, thread-safe internally
**Rationale**:
- Bun is single-threaded per isolate
- Simpler than connection pooling
- Arc<RwLock<>> already used in ForgeDB
- Read locks allow concurrent reads

---

## Task Breakdown - 18 Tasks, 6 Phases

### Phase 1: FFI Bridge Architecture (Tasks 1-3)

**Estimated Total**: 4 hours
**Goal**: Design and scaffold FFI interface

---

#### Task 1.1: Design FFI Interface Contract
**Estimated**: 90 minutes
**Complexity**: Medium
**Prerequisites**: None

**Files to Create**:
- `crates/ffi/include/forgedb.h`
- `crates/ffi/docs/FFI_SPEC.md`

**Deliverables**:

1. **C Header File** (`forgedb.h`):
```c
#ifndef FORGEDB_FFI_H
#define FORGEDB_FFI_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque types
typedef struct ForgeDB ForgeDB;
typedef struct ForgeDBError ForgeDBError;

// Flags for forgedb_open
#define FORGEDB_OPEN_READONLY  0x01
#define FORGEDB_OPEN_CREATE    0x02

// Error codes
#define FORGEDB_OK              0
#define FORGEDB_ERR_IO          1
#define FORGEDB_ERR_NOT_FOUND   2
#define FORGEDB_ERR_INVALID     3
#define FORGEDB_ERR_INTERNAL    4

// Database lifecycle
ForgeDB* forgedb_open(
    const char* path,
    int flags,
    ForgeDBError** error
);

void forgedb_close(ForgeDB* db);

// Read operations
char* forgedb_get(
    ForgeDB* db,
    const char* model,
    const char* id,
    ForgeDBError** error
);

char* forgedb_list(
    ForgeDB* db,
    const char* model,
    const char* filter_json,  // JSON: {"field": "value", ...}
    int32_t limit,
    int32_t offset,
    ForgeDBError** error
);

char* forgedb_query(
    ForgeDB* db,
    const char* model,
    const char* query_json,   // JSON: {"filters": {...}, "sort": [...], ...}
    ForgeDBError** error
);

char* forgedb_get_relations(
    ForgeDB* db,
    const char* model,
    const char* id,
    const char* relation_name,
    ForgeDBError** error
);

// Memory management
void forgedb_free_string(char* str);

// Error handling
int32_t forgedb_error_code(ForgeDBError* error);
const char* forgedb_error_message(ForgeDBError* error);
void forgedb_free_error(ForgeDBError* error);

// Utility
const char* forgedb_version(void);

#ifdef __cplusplus
}
#endif

#endif // FORGEDB_FFI_H
```

2. **FFI Specification Document** (`FFI_SPEC.md`):
```markdown
# ForgeDB FFI Specification

## Memory Ownership Rules

### Database Handle
- **Creation**: `forgedb_open()` returns handle, caller owns
- **Usage**: Handle must be passed to all operations
- **Cleanup**: Caller must call `forgedb_close()` exactly once
- **Thread Safety**: Handle is thread-safe, concurrent reads allowed

### Returned Strings
- **Ownership**: Caller owns all returned strings
- **Cleanup**: Caller must call `forgedb_free_string()` for each non-NULL return
- **Encoding**: All strings are UTF-8
- **Null-terminated**: All strings are null-terminated C strings

### Error Objects
- **Creation**: Functions set error out-parameter on failure
- **Ownership**: Caller owns error object
- **Cleanup**: Caller must call `forgedb_free_error()`
- **Inspection**: Safe to read while owned

## Thread Safety

- **Database Handle**: Thread-safe for concurrent reads
- **Read Operations**: Multiple threads can call simultaneously
- **Error Objects**: Not thread-safe (one per thread recommended)
- **Strings**: Immutable once returned

## Error Handling Pattern

```c
ForgeDBError* err = NULL;
char* result = forgedb_get(db, "User", "123", &err);

if (result == NULL) {
    if (err != NULL) {
        fprintf(stderr, "Error %d: %s\n",
            forgedb_error_code(err),
            forgedb_error_message(err));
        forgedb_free_error(err);
    }
    return;
}

// Use result
printf("Result: %s\n", result);
forgedb_free_string(result);
```

## Return Value Conventions

- **NULL**: Indicates error or not found
- **Non-NULL**: Success, valid data
- **Empty String**: Valid return (e.g., empty list: "[]")
- **Error Parameter**: Set to non-NULL on error, NULL on success
```

**Tests**:
- Validate header compiles with C compiler
- Validate header compiles with C++ compiler
- Document all edge cases

**Success Criteria**:
- ✅ Complete C header file
- ✅ Comprehensive specification document
- ✅ Memory ownership rules documented
- ✅ Thread safety guarantees documented

---

#### Task 1.2: Create Rust FFI Crate Structure
**Estimated**: 60 minutes
**Complexity**: Low
**Prerequisites**: Task 1.1

**Files to Create**:
- `crates/ffi/Cargo.toml`
- `crates/ffi/src/lib.rs`
- `crates/ffi/src/handles.rs`
- `crates/ffi/src/errors.rs`
- `crates/ffi/src/conversions.rs`
- `crates/ffi/build.rs`

**Changes**:

1. **Cargo.toml**:
```toml
[package]
name = "forgedb-ffi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "staticlib"]

[dependencies]
sinkdb-storage = { path = "../storage" }
sinkdb-parser = { path = "../parser" }
libc = "0.2"
serde_json = "1.0"
parking_lot = "0.12"
lazy_static = "1.4"
thiserror = "1.0"

[build-dependencies]
cbindgen = "0.26"

[dev-dependencies]
tempfile = "3.8"
```

2. **build.rs** (generates C header):
```rust
use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_language(cbindgen::Language::C)
        .with_pragma_once(true)
        .with_include_guard("FORGEDB_FFI_H")
        .with_documentation(true)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("include/forgedb.h");
}
```

3. **lib.rs** (skeleton):
```rust
//! ForgeDB FFI Bindings
//!
//! This crate provides C-compatible FFI bindings for ForgeDB,
//! enabling direct database access from Bun and other runtimes.

#![deny(unsafe_op_in_unsafe_fn)]

mod handles;
mod errors;
mod conversions;

pub use handles::*;
pub use errors::*;
pub use conversions::*;

use libc::c_char;
use std::ptr;

/// Opaque database handle
#[repr(C)]
pub struct ForgeDB {
    _private: [u8; 0],
}

/// Opaque error handle
#[repr(C)]
pub struct ForgeDBError {
    _private: [u8; 0],
}

// Export functions (implemented in later tasks)
#[no_mangle]
pub extern "C" fn forgedb_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let version = unsafe {
            std::ffi::CStr::from_ptr(forgedb_version())
        };
        assert!(!version.to_bytes().is_empty());
    }
}
```

4. **Add to workspace** (`Cargo.toml` in root):
```toml
[workspace]
members = [
    "crates/storage",
    "crates/parser",
    # ... existing crates
    "crates/ffi",
]
```

**Tests**:
- Crate compiles as cdylib
- Header file generated correctly
- Version function works

**Success Criteria**:
- ✅ Crate structure created
- ✅ Builds successfully
- ✅ Generates C header
- ✅ Basic test passes

---

#### Task 1.3: Implement Handle Management System
**Estimated**: 90 minutes
**Complexity**: High
**Prerequisites**: Task 1.2

**Files to Modify**:
- `crates/ffi/src/handles.rs`

**Implementation**:

```rust
//! Handle management for FFI
//!
//! Provides safe handle-based access to Rust objects across FFI boundary.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Thread-safe registry for managing opaque handles
pub struct HandleRegistry<T> {
    next_id: AtomicUsize,
    handles: Arc<RwLock<HashMap<usize, Arc<T>>>>,
}

impl<T> HandleRegistry<T> {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            next_id: AtomicUsize::new(1), // Start at 1, reserve 0 for NULL
            handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a value and return an opaque handle
    ///
    /// Returns a pointer that's actually just an ID cast to pointer.
    /// The actual data stays in Rust.
    pub fn insert(&self, value: T) -> *mut T {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let arc = Arc::new(value);

        self.handles.write().insert(id, arc);

        id as *mut T
    }

    /// Get a value by handle
    ///
    /// Returns None if handle is invalid or has been removed.
    pub fn get(&self, handle: *mut T) -> Option<Arc<T>> {
        if handle.is_null() {
            return None;
        }

        let id = handle as usize;
        self.handles.read().get(&id).cloned()
    }

    /// Remove and drop a handle
    ///
    /// After this call, the handle is invalid.
    /// Safe to call multiple times (subsequent calls are no-op).
    pub fn remove(&self, handle: *mut T) -> bool {
        if handle.is_null() {
            return false;
        }

        let id = handle as usize;
        self.handles.write().remove(&id).is_some()
    }

    /// Check if a handle is valid
    pub fn is_valid(&self, handle: *mut T) -> bool {
        if handle.is_null() {
            return false;
        }

        let id = handle as usize;
        self.handles.read().contains_key(&id)
    }

    /// Get the number of active handles
    pub fn len(&self) -> usize {
        self.handles.read().len()
    }
}

impl<T> Default for HandleRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Global registries
lazy_static::lazy_static! {
    pub static ref DB_HANDLES: HandleRegistry<DatabaseHandle> = HandleRegistry::new();
    pub static ref ERROR_HANDLES: HandleRegistry<ErrorHandle> = HandleRegistry::new();
}

/// Internal database handle (never exposed directly)
pub struct DatabaseHandle {
    pub db: Arc<RwLock<sinkdb_storage::Database>>,
    pub path: String,
}

/// Internal error handle (never exposed directly)
pub struct ErrorHandle {
    pub code: i32,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let registry = HandleRegistry::<i32>::new();

        let handle = registry.insert(42);
        assert!(!handle.is_null());

        let value = registry.get(handle).unwrap();
        assert_eq!(*value, 42);
    }

    #[test]
    fn test_remove() {
        let registry = HandleRegistry::<i32>::new();

        let handle = registry.insert(42);
        assert!(registry.is_valid(handle));

        assert!(registry.remove(handle));
        assert!(!registry.is_valid(handle));

        // Second remove returns false
        assert!(!registry.remove(handle));
    }

    #[test]
    fn test_null_handle() {
        let registry = HandleRegistry::<i32>::new();

        assert!(registry.get(std::ptr::null_mut()).is_none());
        assert!(!registry.is_valid(std::ptr::null_mut()));
        assert!(!registry.remove(std::ptr::null_mut()));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let registry = Arc::new(HandleRegistry::<i32>::new());
        let handle = registry.insert(42);

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let registry = registry.clone();
                let handle = handle;
                thread::spawn(move || {
                    for _ in 0..100 {
                        let value = registry.get(handle).unwrap();
                        assert_eq!(*value, 42);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_multiple_handles() {
        let registry = HandleRegistry::<String>::new();

        let h1 = registry.insert("first".to_string());
        let h2 = registry.insert("second".to_string());
        let h3 = registry.insert("third".to_string());

        assert_eq!(registry.len(), 3);

        assert_eq!(*registry.get(h1).unwrap(), "first");
        assert_eq!(*registry.get(h2).unwrap(), "second");
        assert_eq!(*registry.get(h3).unwrap(), "third");

        registry.remove(h2);
        assert_eq!(registry.len(), 2);

        assert!(registry.get(h2).is_none());
        assert!(registry.get(h1).is_some());
        assert!(registry.get(h3).is_some());
    }
}
```

**Tests**:
- Insert and retrieve handles
- Remove handles
- Null handle handling
- Concurrent access safety
- Multiple handles independence

**Success Criteria**:
- ✅ All tests pass
- ✅ Thread-safe verified
- ✅ No memory leaks
- ✅ Handles validated correctly

---

### Phase 2: Core FFI Functions (Tasks 4-7)

**Estimated Total**: 5 hours
**Goal**: Implement database operations

---

#### Task 2.1: Implement Error Handling System
**Estimated**: 60 minutes
**Complexity**: Medium
**Prerequisites**: Task 1.3

**Files to Modify**:
- `crates/ffi/src/errors.rs`
- `crates/ffi/src/lib.rs`

**Implementation**:

```rust
//! Error handling for FFI
//!
//! Provides C-compatible error reporting.

use crate::handles::{ErrorHandle, ERROR_HANDLES};
use crate::ForgeDBError;
use libc::c_char;
use std::ffi::CString;
use std::ptr;

// Error codes (matching C header)
pub const FORGEDB_OK: i32 = 0;
pub const FORGEDB_ERR_IO: i32 = 1;
pub const FORGEDB_ERR_NOT_FOUND: i32 = 2;
pub const FORGEDB_ERR_INVALID: i32 = 3;
pub const FORGEDB_ERR_INTERNAL: i32 = 4;

/// Create an error handle from code and message
pub fn create_error(code: i32, message: String) -> *mut ForgeDBError {
    let handle = ErrorHandle { code, message };
    ERROR_HANDLES.insert(handle) as *mut ForgeDBError
}

/// Set error output parameter if not null
pub fn set_error(error_out: *mut *mut ForgeDBError, code: i32, message: String) {
    if !error_out.is_null() {
        unsafe {
            *error_out = create_error(code, message);
        }
    }
}

/// Get error code from error handle
#[no_mangle]
pub extern "C" fn forgedb_error_code(error: *mut ForgeDBError) -> i32 {
    if let Some(err) = ERROR_HANDLES.get(error as *mut ErrorHandle) {
        err.code
    } else {
        FORGEDB_ERR_INVALID
    }
}

/// Get error message from error handle
///
/// Returns a pointer to internal storage. Valid until error is freed.
#[no_mangle]
pub extern "C" fn forgedb_error_message(error: *mut ForgeDBError) -> *const c_char {
    if let Some(err) = ERROR_HANDLES.get(error as *mut ErrorHandle) {
        // Safety: message is valid UTF-8 and null-terminated
        err.message.as_ptr() as *const c_char
    } else {
        b"Invalid error handle\0".as_ptr() as *const c_char
    }
}

/// Free an error handle
#[no_mangle]
pub extern "C" fn forgedb_free_error(error: *mut ForgeDBError) {
    if !error.is_null() {
        ERROR_HANDLES.remove(error as *mut ErrorHandle);
    }
}

/// Helper macro for error handling in FFI functions
#[macro_export]
macro_rules! ffi_try {
    ($expr:expr, $error_out:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                let (code, msg) = match e {
                    _ if e.to_string().contains("not found") => {
                        (FORGEDB_ERR_NOT_FOUND, e.to_string())
                    }
                    _ if e.to_string().contains("IO") => {
                        (FORGEDB_ERR_IO, e.to_string())
                    }
                    _ => (FORGEDB_ERR_INTERNAL, e.to_string()),
                };
                $crate::errors::set_error($error_out, code, msg);
                return std::ptr::null_mut();
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_error() {
        let err = create_error(FORGEDB_ERR_IO, "Test error".to_string());
        assert!(!err.is_null());

        let code = forgedb_error_code(err);
        assert_eq!(code, FORGEDB_ERR_IO);

        let msg = unsafe {
            std::ffi::CStr::from_ptr(forgedb_error_message(err))
        };
        assert_eq!(msg.to_str().unwrap(), "Test error");

        forgedb_free_error(err);
    }

    #[test]
    fn test_null_error() {
        let code = forgedb_error_code(ptr::null_mut());
        assert_eq!(code, FORGEDB_ERR_INVALID);

        let msg = unsafe {
            std::ffi::CStr::from_ptr(forgedb_error_message(ptr::null_mut()))
        };
        assert_eq!(msg.to_str().unwrap(), "Invalid error handle");

        // Should not crash
        forgedb_free_error(ptr::null_mut());
    }
}
```

**Tests**:
- Create and inspect errors
- Null error handling
- Error code mapping
- Memory cleanup

**Success Criteria**:
- ✅ Error creation works
- ✅ Error inspection works
- ✅ Null-safe operations
- ✅ No memory leaks

---

#### Task 2.2: Implement String Conversions
**Estimated**: 45 minutes
**Complexity**: Medium
**Prerequisites**: Task 2.1

**Files to Modify**:
- `crates/ffi/src/conversions.rs`

**Implementation**:

```rust
//! Type conversions between Rust and C
//!
//! Handles string marshalling and JSON serialization.

use libc::c_char;
use std::ffi::{CStr, CString};
use std::ptr;

/// Convert C string to Rust string
///
/// Returns None if pointer is null or not valid UTF-8.
pub fn c_str_to_rust(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .ok()
            .map(|s| s.to_string())
    }
}

/// Convert Rust string to C string
///
/// Returns a pointer that must be freed with forgedb_free_string.
/// Returns null if allocation fails.
pub fn rust_str_to_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Free a C string returned by ForgeDB
#[no_mangle]
pub extern "C" fn forgedb_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
            // Dropped here, memory freed
        }
    }
}

/// Serialize Rust value to JSON C string
pub fn to_json_string<T: serde::Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => rust_str_to_c(json),
        Err(_) => ptr::null_mut(),
    }
}

/// Deserialize JSON C string to Rust value
pub fn from_json_string<T: serde::de::DeserializeOwned>(ptr: *const c_char) -> Option<T> {
    let json = c_str_to_rust(ptr)?;
    serde_json::from_str(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_to_rust() {
        let c_str = CString::new("hello").unwrap();
        let rust_str = c_str_to_rust(c_str.as_ptr()).unwrap();
        assert_eq!(rust_str, "hello");
    }

    #[test]
    fn test_rust_to_c() {
        let rust_str = "hello".to_string();
        let c_str = rust_str_to_c(rust_str);
        assert!(!c_str.is_null());

        let result = unsafe {
            CStr::from_ptr(c_str).to_str().unwrap()
        };
        assert_eq!(result, "hello");

        forgedb_free_string(c_str);
    }

    #[test]
    fn test_null_handling() {
        assert!(c_str_to_rust(ptr::null()).is_none());
        forgedb_free_string(ptr::null_mut()); // Should not crash
    }

    #[test]
    fn test_json_round_trip() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Test {
            value: i32,
            name: String,
        }

        let test = Test {
            value: 42,
            name: "test".to_string(),
        };

        let json_str = to_json_string(&test);
        assert!(!json_str.is_null());

        let result: Test = from_json_string(json_str).unwrap();
        assert_eq!(result, test);

        forgedb_free_string(json_str);
    }
}
```

**Tests**:
- C to Rust conversion
- Rust to C conversion
- JSON serialization
- Memory cleanup

**Success Criteria**:
- ✅ Conversions work correctly
- ✅ UTF-8 handling correct
- ✅ JSON round-trip works
- ✅ No memory leaks

---

#### Task 2.3: Implement Database Open/Close
**Estimated**: 75 minutes
**Complexity**: Medium
**Prerequisites**: Task 2.2

**Files to Modify**:
- `crates/ffi/src/lib.rs`

**Implementation**:

```rust
use crate::conversions::*;
use crate::errors::*;
use crate::handles::*;
use libc::{c_char, c_int};
use std::ptr;
use std::sync::Arc;
use parking_lot::RwLock;

// Flags
pub const FORGEDB_OPEN_READONLY: c_int = 0x01;
pub const FORGEDB_OPEN_CREATE: c_int = 0x02;

/// Open a ForgeDB database
///
/// # Parameters
/// - `path`: Path to database directory (null-terminated C string)
/// - `flags`: Bitwise OR of FORGEDB_OPEN_* flags
/// - `error`: Output parameter for error (can be NULL)
///
/// # Returns
/// - Non-NULL handle on success
/// - NULL on error (check error parameter)
///
/// # Example
/// ```c
/// ForgeDBError* err = NULL;
/// ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, &err);
/// if (db == NULL) {
///     fprintf(stderr, "Error: %s\n", forgedb_error_message(err));
///     forgedb_free_error(err);
///     return 1;
/// }
/// ```
#[no_mangle]
pub extern "C" fn forgedb_open(
    path: *const c_char,
    flags: c_int,
    error: *mut *mut ForgeDBError,
) -> *mut ForgeDB {
    // Convert path
    let path_str = match c_str_to_rust(path) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid path".to_string());
            return ptr::null_mut();
        }
    };

    // Parse flags
    let readonly = (flags & FORGEDB_OPEN_READONLY) != 0;
    let create = (flags & FORGEDB_OPEN_CREATE) != 0;

    // Open database
    let db = ffi_try!(
        sinkdb_storage::Database::open(&path_str, readonly, create),
        error
    );

    // Create handle
    let handle = DatabaseHandle {
        db: Arc::new(RwLock::new(db)),
        path: path_str,
    };

    DB_HANDLES.insert(handle) as *mut ForgeDB
}

/// Close a ForgeDB database
///
/// After this call, the handle is invalid and must not be used.
/// Safe to call with NULL or already-closed handle.
///
/// # Example
/// ```c
/// forgedb_close(db);
/// db = NULL;  // Good practice
/// ```
#[no_mangle]
pub extern "C" fn forgedb_close(db: *mut ForgeDB) {
    if !db.is_null() {
        DB_HANDLES.remove(db as *mut DatabaseHandle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_open_close() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.db");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();

        let mut err: *mut ForgeDBError = ptr::null_mut();
        let db = forgedb_open(
            path_c.as_ptr(),
            FORGEDB_OPEN_CREATE,
            &mut err,
        );

        assert!(!db.is_null());
        assert!(err.is_null());

        forgedb_close(db);
    }

    #[test]
    fn test_open_nonexistent() {
        let path_c = CString::new("/nonexistent/path/db").unwrap();

        let mut err: *mut ForgeDBError = ptr::null_mut();
        let db = forgedb_open(
            path_c.as_ptr(),
            FORGEDB_OPEN_READONLY,
            &mut err,
        );

        assert!(db.is_null());
        assert!(!err.is_null());

        let code = forgedb_error_code(err);
        assert!(code != FORGEDB_OK);

        forgedb_free_error(err);
    }

    #[test]
    fn test_close_null() {
        // Should not crash
        forgedb_close(ptr::null_mut());
    }

    #[test]
    fn test_double_close() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.db");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();

        let db = forgedb_open(
            path_c.as_ptr(),
            FORGEDB_OPEN_CREATE,
            ptr::null_mut(),
        );

        forgedb_close(db);
        forgedb_close(db);  // Should be safe
    }
}
```

**Tests**:
- Open valid database
- Open non-existent path
- Close database
- Double close safety
- Null handle handling

**Success Criteria**:
- ✅ Database opens correctly
- ✅ Error handling works
- ✅ Close is safe
- ✅ No memory leaks

---

#### Task 2.4: Implement Get Operation
**Estimated**: 75 minutes
**Complexity**: Medium
**Prerequisites**: Task 2.3

**Files to Modify**:
- `crates/ffi/src/lib.rs`

**Implementation**:

```rust
/// Get a single record by ID
///
/// # Parameters
/// - `db`: Database handle
/// - `model`: Model name (e.g., "User")
/// - `id`: Record ID
/// - `error`: Output parameter for error (can be NULL)
///
/// # Returns
/// - JSON string on success (must be freed with forgedb_free_string)
/// - NULL on error or not found
///
/// # Example
/// ```c
/// char* json = forgedb_get(db, "User", "123", &err);
/// if (json != NULL) {
///     printf("User: %s\n", json);
///     forgedb_free_string(json);
/// }
/// ```
#[no_mangle]
pub extern "C" fn forgedb_get(
    db: *mut ForgeDB,
    model: *const c_char,
    id: *const c_char,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    let db_handle = match DB_HANDLES.get(db as *mut DatabaseHandle) {
        Some(h) => h,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid database handle".to_string());
            return ptr::null_mut();
        }
    };

    // Convert parameters
    let model_str = match c_str_to_rust(model) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid model name".to_string());
            return ptr::null_mut();
        }
    };

    let id_str = match c_str_to_rust(id) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid id".to_string());
            return ptr::null_mut();
        }
    };

    // Get from database
    let db = db_handle.db.read();
    let result = ffi_try!(
        db.get(&model_str, &id_str),
        error
    );

    // Serialize to JSON
    to_json_string(&result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::ffi::CString;

    fn setup_test_db() -> (*mut ForgeDB, TempDir) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.db");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();

        let db = forgedb_open(
            path_c.as_ptr(),
            FORGEDB_OPEN_CREATE,
            ptr::null_mut(),
        );

        (db, temp)
    }

    #[test]
    fn test_get_existing() {
        let (db, _temp) = setup_test_db();

        // TODO: Insert test data first

        let model = CString::new("User").unwrap();
        let id = CString::new("123").unwrap();

        let mut err: *mut ForgeDBError = ptr::null_mut();
        let json = forgedb_get(
            db,
            model.as_ptr(),
            id.as_ptr(),
            &mut err,
        );

        // May be null if record doesn't exist (not an error)
        if !json.is_null() {
            let json_str = unsafe {
                CStr::from_ptr(json).to_str().unwrap()
            };
            println!("Got JSON: {}", json_str);
            forgedb_free_string(json);
        }

        forgedb_close(db);
    }

    #[test]
    fn test_get_invalid_handle() {
        let model = CString::new("User").unwrap();
        let id = CString::new("123").unwrap();

        let mut err: *mut ForgeDBError = ptr::null_mut();
        let json = forgedb_get(
            ptr::null_mut(),
            model.as_ptr(),
            id.as_ptr(),
            &mut err,
        );

        assert!(json.is_null());
        assert!(!err.is_null());

        let code = forgedb_error_code(err);
        assert_eq!(code, FORGEDB_ERR_INVALID);

        forgedb_free_error(err);
    }

    #[test]
    fn test_get_null_params() {
        let (db, _temp) = setup_test_db();

        let model = CString::new("User").unwrap();

        let mut err: *mut ForgeDBError = ptr::null_mut();
        let json = forgedb_get(
            db,
            model.as_ptr(),
            ptr::null(),  // NULL id
            &mut err,
        );

        assert!(json.is_null());
        assert!(!err.is_null());

        forgedb_free_error(err);
        forgedb_close(db);
    }
}
```

**Tests**:
- Get existing record
- Get non-existent record
- Invalid handle
- Null parameters
- Memory cleanup

**Success Criteria**:
- ✅ Get operation works
- ✅ JSON serialization correct
- ✅ Error handling robust
- ✅ No memory leaks

---

#### Task 2.5: Implement List Operation
**Estimated**: 75 minutes
**Complexity**: Medium
**Prerequisites**: Task 2.4

**Files to Modify**:
- `crates/ffi/src/lib.rs`

**Implementation**:

```rust
/// List records with optional filtering
///
/// # Parameters
/// - `db`: Database handle
/// - `model`: Model name
/// - `filter_json`: JSON object with filters (can be NULL for no filter)
///   Example: `{"email": "test@example.com", "age": 25}`
/// - `limit`: Maximum number of records (0 for all)
/// - `offset`: Number of records to skip (0 for none)
/// - `error`: Output parameter for error
///
/// # Returns
/// - JSON array string on success (must be freed)
/// - NULL on error
///
/// # Example
/// ```c
/// // List first 10 users
/// char* json = forgedb_list(db, "User", NULL, 10, 0, &err);
/// if (json != NULL) {
///     printf("Users: %s\n", json);
///     forgedb_free_string(json);
/// }
/// ```
#[no_mangle]
pub extern "C" fn forgedb_list(
    db: *mut ForgeDB,
    model: *const c_char,
    filter_json: *const c_char,
    limit: i32,
    offset: i32,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    let db_handle = match DB_HANDLES.get(db as *mut DatabaseHandle) {
        Some(h) => h,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid database handle".to_string());
            return ptr::null_mut();
        }
    };

    // Convert model name
    let model_str = match c_str_to_rust(model) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid model name".to_string());
            return ptr::null_mut();
        }
    };

    // Parse filters (optional)
    let filters: Option<serde_json::Value> = if filter_json.is_null() {
        None
    } else {
        match from_json_string(filter_json) {
            Some(f) => Some(f),
            None => {
                set_error(error, FORGEDB_ERR_INVALID, "Invalid filter JSON".to_string());
                return ptr::null_mut();
            }
        }
    };

    // Build query
    let query = sinkdb_storage::Query {
        filters: filters.unwrap_or_default(),
        limit: if limit > 0 { Some(limit as usize) } else { None },
        offset: if offset > 0 { Some(offset as usize) } else { None },
        ..Default::default()
    };

    // Execute query
    let db = db_handle.db.read();
    let results = ffi_try!(
        db.list(&model_str, query),
        error
    );

    // Serialize to JSON array
    to_json_string(&results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_all() {
        let (db, _temp) = setup_test_db();

        let model = CString::new("User").unwrap();

        let mut err: *mut ForgeDBError = ptr::null_mut();
        let json = forgedb_list(
            db,
            model.as_ptr(),
            ptr::null(),  // No filters
            0,            // No limit
            0,            // No offset
            &mut err,
        );

        if !json.is_null() {
            let json_str = unsafe {
                CStr::from_ptr(json).to_str().unwrap()
            };
            println!("List result: {}", json_str);

            // Should be valid JSON array
            let _parsed: Vec<serde_json::Value> = serde_json::from_str(json_str).unwrap();

            forgedb_free_string(json);
        }

        forgedb_close(db);
    }

    #[test]
    fn test_list_with_limit() {
        let (db, _temp) = setup_test_db();

        let model = CString::new("User").unwrap();

        let json = forgedb_list(
            db,
            model.as_ptr(),
            ptr::null(),
            10,  // Limit 10
            0,
            ptr::null_mut(),
        );

        if !json.is_null() {
            let json_str = unsafe {
                CStr::from_ptr(json).to_str().unwrap()
            };
            let parsed: Vec<serde_json::Value> = serde_json::from_str(json_str).unwrap();
            assert!(parsed.len() <= 10);

            forgedb_free_string(json);
        }

        forgedb_close(db);
    }

    #[test]
    fn test_list_with_filters() {
        let (db, _temp) = setup_test_db();

        let model = CString::new("User").unwrap();
        let filters = CString::new(r#"{"verified": true}"#).unwrap();

        let json = forgedb_list(
            db,
            model.as_ptr(),
            filters.as_ptr(),
            0,
            0,
            ptr::null_mut(),
        );

        if !json.is_null() {
            forgedb_free_string(json);
        }

        forgedb_close(db);
    }
}
```

**Tests**:
- List all records
- List with limit
- List with offset
- List with filters
- Invalid filter JSON

**Success Criteria**:
- ✅ List operation works
- ✅ Filters applied correctly
- ✅ Pagination works
- ✅ No memory leaks

---

#### Task 2.6: Implement Query Operation
**Estimated**: 60 minutes
**Complexity**: Medium
**Prerequisites**: Task 2.5

**Files to Modify**:
- `crates/ffi/src/lib.rs`

**Implementation**:

```rust
/// Execute complex query
///
/// # Parameters
/// - `db`: Database handle
/// - `model`: Model name
/// - `query_json`: JSON query object
///   Example: `{"filters": {"age": {"gt": 18}}, "sort": ["name"], "limit": 10}`
/// - `error`: Output parameter for error
///
/// # Returns
/// - JSON array string on success
/// - NULL on error
#[no_mangle]
pub extern "C" fn forgedb_query(
    db: *mut ForgeDB,
    model: *const c_char,
    query_json: *const c_char,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    let db_handle = match DB_HANDLES.get(db as *mut DatabaseHandle) {
        Some(h) => h,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid database handle".to_string());
            return ptr::null_mut();
        }
    };

    // Convert parameters
    let model_str = match c_str_to_rust(model) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid model name".to_string());
            return ptr::null_mut();
        }
    };

    let query: sinkdb_storage::Query = match from_json_string(query_json) {
        Some(q) => q,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid query JSON".to_string());
            return ptr::null_mut();
        }
    };

    // Execute query
    let db = db_handle.db.read();
    let results = ffi_try!(
        db.query(&model_str, query),
        error
    );

    to_json_string(&results)
}
```

---

#### Task 2.7: Implement Relation Traversal
**Estimated**: 60 minutes
**Complexity**: Medium
**Prerequisites**: Task 2.6

**Files to Modify**:
- `crates/ffi/src/lib.rs`

**Implementation**:

```rust
/// Get related records
///
/// # Parameters
/// - `db`: Database handle
/// - `model`: Model name (e.g., "User")
/// - `id`: Record ID
/// - `relation_name`: Name of relation field (e.g., "posts")
/// - `error`: Output parameter for error
///
/// # Returns
/// - JSON array of related records
/// - NULL on error
///
/// # Example
/// ```c
/// // Get all posts for user 123
/// char* json = forgedb_get_relations(db, "User", "123", "posts", &err);
/// if (json != NULL) {
///     printf("Posts: %s\n", json);
///     forgedb_free_string(json);
/// }
/// ```
#[no_mangle]
pub extern "C" fn forgedb_get_relations(
    db: *mut ForgeDB,
    model: *const c_char,
    id: *const c_char,
    relation_name: *const c_char,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    let db_handle = match DB_HANDLES.get(db as *mut DatabaseHandle) {
        Some(h) => h,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid database handle".to_string());
            return ptr::null_mut();
        }
    };

    // Convert parameters
    let model_str = match c_str_to_rust(model) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid model name".to_string());
            return ptr::null_mut();
        }
    };

    let id_str = match c_str_to_rust(id) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid id".to_string());
            return ptr::null_mut();
        }
    };

    let relation_str = match c_str_to_rust(relation_name) {
        Some(s) => s,
        None => {
            set_error(error, FORGEDB_ERR_INVALID, "Invalid relation name".to_string());
            return ptr::null_mut();
        }
    };

    // Get relations
    let db = db_handle.db.read();
    let results = ffi_try!(
        db.get_relations(&model_str, &id_str, &relation_str),
        error
    );

    to_json_string(&results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_relations() {
        let (db, _temp) = setup_test_db();

        let model = CString::new("User").unwrap();
        let id = CString::new("123").unwrap();
        let relation = CString::new("posts").unwrap();

        let json = forgedb_get_relations(
            db,
            model.as_ptr(),
            id.as_ptr(),
            relation.as_ptr(),
            ptr::null_mut(),
        );

        if !json.is_null() {
            let json_str = unsafe {
                CStr::from_ptr(json).to_str().unwrap()
            };
            let _parsed: Vec<serde_json::Value> = serde_json::from_str(json_str).unwrap();
            forgedb_free_string(json);
        }

        forgedb_close(db);
    }
}
```

**Tests**:
- Get existing relations
- Get non-existent relation
- Invalid parameters

**Success Criteria**:
- ✅ Relation traversal works
- ✅ Returns array
- ✅ Handles missing relations
- ✅ No memory leaks

---

### Phase 3: Bun TypeScript Bindings (Tasks 8-11)

**Estimated Total**: 6 hours
**Goal**: TypeScript wrapper for FFI

---

#### Task 3.1: Generate TypeScript FFI Declarations
**Estimated**: 75 minutes
**Complexity**: Medium
**Prerequisites**: Phase 2 complete

**Files to Create**:
- `runtime/bun/ffi/forgedb-ffi.ts`
- `runtime/bun/ffi/types.ts`

**Implementation**:

```typescript
// runtime/bun/ffi/forgedb-ffi.ts

import { dlopen, FFIType, suffix, CString, ptr } from "bun:ffi";
import { join } from "path";

// Find the shared library
function findLibrary(): string {
  const libName = `libforgedb_ffi.${suffix}`;

  // Check common locations
  const locations = [
    join(process.cwd(), "target", "debug", libName),
    join(process.cwd(), "target", "release", libName),
    join(process.cwd(), "lib", libName),
    libName, // System path
  ];

  for (const location of locations) {
    try {
      // Try to open each location
      const lib = dlopen(location, {});
      lib.close();
      return location;
    } catch {
      continue;
    }
  }

  throw new Error(`Could not find ${libName} in: ${locations.join(", ")}`);
}

// FFI symbol declarations
const lib = dlopen(findLibrary(), {
  // Version
  forgedb_version: {
    args: [],
    returns: FFIType.cstring,
  },

  // Database lifecycle
  forgedb_open: {
    args: [FFIType.cstring, FFIType.i32, FFIType.ptr],
    returns: FFIType.ptr,
  },
  forgedb_close: {
    args: [FFIType.ptr],
    returns: FFIType.void,
  },

  // Read operations
  forgedb_get: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.ptr],
    returns: FFIType.cstring,
  },
  forgedb_list: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.i32, FFIType.i32, FFIType.ptr],
    returns: FFIType.cstring,
  },
  forgedb_query: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.ptr],
    returns: FFIType.cstring,
  },
  forgedb_get_relations: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.cstring, FFIType.ptr],
    returns: FFIType.cstring,
  },

  // Memory management
  forgedb_free_string: {
    args: [FFIType.ptr],
    returns: FFIType.void,
  },

  // Error handling
  forgedb_error_code: {
    args: [FFIType.ptr],
    returns: FFIType.i32,
  },
  forgedb_error_message: {
    args: [FFIType.ptr],
    returns: FFIType.cstring,
  },
  forgedb_free_error: {
    args: [FFIType.ptr],
    returns: FFIType.void,
  },
});

export const { symbols } = lib;

// Export types
export type ForgeDBHandle = pointer;
export type ForgeDBErrorHandle = pointer;

// Constants
export const FORGEDB_OPEN_READONLY = 0x01;
export const FORGEDB_OPEN_CREATE = 0x02;

export const FORGEDB_OK = 0;
export const FORGEDB_ERR_IO = 1;
export const FORGEDB_ERR_NOT_FOUND = 2;
export const FORGEDB_ERR_INVALID = 3;
export const FORGEDB_ERR_INTERNAL = 4;
```

```typescript
// runtime/bun/ffi/types.ts

export interface DatabaseOptions {
  readOnly?: boolean;
  create?: boolean;
}

export interface QueryOptions {
  filters?: Record<string, any>;
  sort?: string[];
  limit?: number;
  offset?: number;
}

export class ForgeDBError extends Error {
  constructor(
    public code: number,
    message: string
  ) {
    super(message);
    this.name = "ForgeDBError";
  }
}
```

**Tests**:
- Library loads correctly
- Symbols are exported
- Constants defined

**Success Criteria**:
- ✅ FFI declarations compile
- ✅ Library loads
- ✅ All symbols available

---

#### Task 3.2: Create Database Class Wrapper
**Estimated**: 90 minutes
**Complexity**: High
**Prerequisites**: Task 3.1

**Files to Create**:
- `runtime/bun/ffi/Database.ts`

**Implementation**:

```typescript
// runtime/bun/ffi/Database.ts

import { ptr, CString } from "bun:ffi";
import {
  symbols,
  ForgeDBHandle,
  ForgeDBErrorHandle,
  FORGEDB_OPEN_READONLY,
  FORGEDB_OPEN_CREATE,
} from "./forgedb-ffi";
import { DatabaseOptions, ForgeDBError } from "./types";

export class Database {
  private handle: ForgeDBHandle;
  private path: string;
  private closed: boolean = false;

  // Static registry for automatic cleanup
  private static registry = new FinalizationRegistry<ForgeDBHandle>(
    (handle: ForgeDBHandle) => {
      if (handle !== null) {
        symbols.forgedb_close(handle);
      }
    }
  );

  constructor(path: string, options: DatabaseOptions = {}) {
    this.path = path;

    // Convert options to flags
    let flags = 0;
    if (options.readOnly) flags |= FORGEDB_OPEN_READONLY;
    if (options.create) flags |= FORGEDB_OPEN_CREATE;

    // Open database
    const errorPtr = new BigUint64Array(1);
    const pathCStr = new CString(path);

    this.handle = symbols.forgedb_open(
      pathCStr,
      flags,
      ptr(errorPtr)
    );

    // Check for errors
    if (this.handle === null) {
      const errorHandle = errorPtr[0];
      if (errorHandle !== 0n) {
        const code = symbols.forgedb_error_code(errorHandle);
        const messageCStr = symbols.forgedb_error_message(errorHandle);
        const message = new CString(messageCStr).toString();
        symbols.forgedb_free_error(errorHandle);
        throw new ForgeDBError(code, message);
      } else {
        throw new ForgeDBError(-1, "Failed to open database");
      }
    }

    // Register for automatic cleanup
    Database.registry.register(this, this.handle, this);
  }

  /**
   * Get a single record by ID
   */
  async get<T = any>(model: string, id: string): Promise<T | null> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelCStr = new CString(model);
    const idCStr = new CString(id);

    const resultPtr = symbols.forgedb_get(
      this.handle,
      modelCStr,
      idCStr,
      ptr(errorPtr)
    );

    if (resultPtr === null) {
      const errorHandle = errorPtr[0];
      if (errorHandle !== 0n) {
        const code = symbols.forgedb_error_code(errorHandle);

        // NOT_FOUND is not an error, just return null
        if (code === 2) { // FORGEDB_ERR_NOT_FOUND
          symbols.forgedb_free_error(errorHandle);
          return null;
        }

        const message = new CString(symbols.forgedb_error_message(errorHandle)).toString();
        symbols.forgedb_free_error(errorHandle);
        throw new ForgeDBError(code, message);
      }
      return null;
    }

    try {
      const json = new CString(resultPtr).toString();
      return JSON.parse(json) as T;
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * List records with optional filters
   */
  async list<T = any>(
    model: string,
    filters?: Record<string, any>,
    limit: number = 0,
    offset: number = 0
  ): Promise<T[]> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelCStr = new CString(model);
    const filtersCStr = filters ? new CString(JSON.stringify(filters)) : null;

    const resultPtr = symbols.forgedb_list(
      this.handle,
      modelCStr,
      filtersCStr,
      limit,
      offset,
      ptr(errorPtr)
    );

    if (resultPtr === null) {
      this.handleError(errorPtr);
      return [];
    }

    try {
      const json = new CString(resultPtr).toString();
      return JSON.parse(json) as T[];
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * Execute complex query
   */
  async query<T = any>(model: string, query: any): Promise<T[]> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelCStr = new CString(model);
    const queryCStr = new CString(JSON.stringify(query));

    const resultPtr = symbols.forgedb_query(
      this.handle,
      modelCStr,
      queryCStr,
      ptr(errorPtr)
    );

    if (resultPtr === null) {
      this.handleError(errorPtr);
      return [];
    }

    try {
      const json = new CString(resultPtr).toString();
      return JSON.parse(json) as T[];
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * Get related records
   */
  async getRelations<T = any>(
    model: string,
    id: string,
    relationName: string
  ): Promise<T[]> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelCStr = new CString(model);
    const idCStr = new CString(id);
    const relationCStr = new CString(relationName);

    const resultPtr = symbols.forgedb_get_relations(
      this.handle,
      modelCStr,
      idCStr,
      relationCStr,
      ptr(errorPtr)
    );

    if (resultPtr === null) {
      this.handleError(errorPtr);
      return [];
    }

    try {
      const json = new CString(resultPtr).toString();
      return JSON.parse(json) as T[];
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * Close the database
   */
  close(): void {
    if (!this.closed && this.handle !== null) {
      Database.registry.unregister(this);
      symbols.forgedb_close(this.handle);
      this.handle = null;
      this.closed = true;
    }
  }

  /**
   * Check if database is open
   */
  isOpen(): boolean {
    return !this.closed && this.handle !== null;
  }

  private ensureOpen(): void {
    if (this.closed || this.handle === null) {
      throw new Error("Database is closed");
    }
  }

  private handleError(errorPtr: BigUint64Array): void {
    const errorHandle = errorPtr[0];
    if (errorHandle !== 0n) {
      const code = symbols.forgedb_error_code(errorHandle);
      const message = new CString(symbols.forgedb_error_message(errorHandle)).toString();
      symbols.forgedb_free_error(errorHandle);
      throw new ForgeDBError(code, message);
    }
  }
}
```

**Tests**:
- Open database
- Get record
- List records
- Close database
- Error handling

**Success Criteria**:
- ✅ Database class works
- ✅ All operations functional
- ✅ Memory managed correctly
- ✅ Errors handled properly

---

#### Task 3.3: Create Type-Safe Query Builder
**Estimated**: 120 minutes
**Complexity**: High
**Prerequisites**: Task 3.2

**Files to Create**:
- `runtime/bun/ffi/QueryBuilder.ts`

**Implementation**:

```typescript
// runtime/bun/ffi/QueryBuilder.ts

import { Database } from "./Database";

export class QueryBuilder<T = any> {
  private filters: Record<string, any> = {};
  private sortFields: string[] = [];
  private _limit?: number;
  private _offset?: number;

  constructor(
    private db: Database,
    private model: string
  ) {}

  /**
   * Add equality filter
   */
  where(field: string, value: any): this {
    this.filters[field] = value;
    return this;
  }

  /**
   * Add comparison filter
   */
  whereLt(field: string, value: number): this {
    this.filters[field] = { lt: value };
    return this;
  }

  whereLte(field: string, value: number): this {
    this.filters[field] = { lte: value };
    return this;
  }

  whereGt(field: string, value: number): this {
    this.filters[field] = { gt: value };
    return this;
  }

  whereGte(field: string, value: number): this {
    this.filters[field] = { gte: value };
    return this;
  }

  /**
   * Add IN filter
   */
  whereIn(field: string, values: any[]): this {
    this.filters[field] = { in: values };
    return this;
  }

  /**
   * Add sort field
   */
  orderBy(field: string, direction: "asc" | "desc" = "asc"): this {
    const sortField = direction === "desc" ? `-${field}` : field;
    this.sortFields.push(sortField);
    return this;
  }

  /**
   * Set limit
   */
  limit(n: number): this {
    this._limit = n;
    return this;
  }

  /**
   * Set offset
   */
  offset(n: number): this {
    this._offset = n;
    return this;
  }

  /**
   * Execute query and return results
   */
  async execute(): Promise<T[]> {
    const query: any = {};

    if (Object.keys(this.filters).length > 0) {
      query.filters = this.filters;
    }

    if (this.sortFields.length > 0) {
      query.sort = this.sortFields;
    }

    if (this._limit !== undefined) {
      query.limit = this._limit;
    }

    if (this._offset !== undefined) {
      query.offset = this._offset;
    }

    return this.db.query<T>(this.model, query);
  }

  /**
   * Get first result
   */
  async first(): Promise<T | null> {
    const results = await this.limit(1).execute();
    return results.length > 0 ? results[0] : null;
  }

  /**
   * Count results (without fetching data)
   */
  async count(): Promise<number> {
    const results = await this.execute();
    return results.length;
  }
}

// Extend Database class with query builder
declare module "./Database" {
  interface Database {
    query<T>(model: string): QueryBuilder<T>;
  }
}

Database.prototype.query = function<T>(this: Database, model: string): QueryBuilder<T> {
  return new QueryBuilder<T>(this, model);
};
```

**Usage Example**:
```typescript
// Type-safe queries
const users = await db
  .query<User>("User")
  .where("verified", true)
  .whereGt("age", 18)
  .orderBy("createdAt", "desc")
  .limit(10)
  .execute();

const firstUser = await db
  .query<User>("User")
  .where("email", "test@example.com")
  .first();
```

**Tests**:
- Simple where clause
- Multiple filters
- Sorting
- Pagination
- first() method

**Success Criteria**:
- ✅ Query builder works
- ✅ Type-safe API
- ✅ Chainable methods
- ✅ Correct SQL generation

---

#### Task 3.4: Add Automatic Resource Cleanup
**Estimated**: 60 minutes
**Complexity**: Medium
**Prerequisites**: Task 3.2

**Files to Modify**:
- `runtime/bun/ffi/Database.ts`

**Enhancement** (already included in Task 3.2):

```typescript
// Using FinalizationRegistry for automatic cleanup
private static registry = new FinalizationRegistry<ForgeDBHandle>(
  (handle: ForgeDBHandle) => {
    if (handle !== null) {
      symbols.forgedb_close(handle);
    }
  }
);

// Register in constructor
Database.registry.register(this, this.handle, this);

// Unregister on explicit close
close(): void {
  if (!this.closed && this.handle !== null) {
    Database.registry.unregister(this);
    symbols.forgedb_close(this.handle);
    this.handle = null;
    this.closed = true;
  }
}
```

**Additional Tests**:
- Test automatic cleanup on GC
- Test explicit close
- Test double close safety
- Test use-after-close error

**Success Criteria**:
- ✅ Automatic cleanup works
- ✅ No memory leaks
- ✅ Safe against double-free
- ✅ Clear error messages

---

### Phase 4: Integration with Sprint 17 (Tasks 12-14)

**Estimated Total**: 4 hours
**Goal**: Replace HTTP with FFI in Bun server

---

#### Task 4.1: Update db-client.ts to Use FFI
**Estimated**: 90 minutes
**Complexity**: Medium
**Prerequisites**: Phase 3 complete

**Files to Modify**:
- `runtime/bun/src/db-client.ts`

**Implementation**:

```typescript
// runtime/bun/src/db-client.ts

import { Database } from "../ffi/Database";

export interface DBClientConfig {
  // Sprint 17: HTTP endpoint
  apiEndpoint?: string;

  // Sprint 24: FFI path
  dataPath?: string;
  readOnly?: boolean;

  // Auto-detect mode
  mode?: "http" | "ffi" | "auto";
}

export interface DBClient {
  get(model: string, id: string): Promise<any>;
  query(model: string, filters: Record<string, any>): Promise<any[]>;
  getRelations(model: string, id: string, relation: string): Promise<any[]>;
  close?(): void;
}

/**
 * Create a database client
 *
 * Sprint 17 mode (HTTP):
 *   createDBClient({ apiEndpoint: "http://localhost:3000" })
 *
 * Sprint 24 mode (FFI):
 *   createDBClient({ dataPath: "./data", readOnly: true })
 *
 * Auto mode (default):
 *   createDBClient({ mode: "auto" })
 *   - Tries FFI first
 *   - Falls back to HTTP if FFI unavailable
 */
export function createDBClient(config: DBClientConfig): DBClient {
  const mode = config.mode || detectMode(config);

  if (mode === "ffi") {
    return createFFIClient(config);
  } else {
    return createHTTPClient(config);
  }
}

function detectMode(config: DBClientConfig): "http" | "ffi" {
  // If dataPath provided, use FFI
  if (config.dataPath) {
    return "ffi";
  }

  // If apiEndpoint provided, use HTTP
  if (config.apiEndpoint) {
    return "http";
  }

  // Try FFI first (Sprint 24+)
  if (config.mode === "auto") {
    try {
      // Try to load FFI library
      const { Database } = require("../ffi/Database");
      return "ffi";
    } catch {
      // Fall back to HTTP
      return "http";
    }
  }

  // Default to HTTP for backwards compatibility
  return "http";
}

/**
 * FFI-based client (Sprint 24)
 */
function createFFIClient(config: DBClientConfig): DBClient {
  const dataPath = config.dataPath || process.env.FORGEDB_DATA || "./data";
  const db = new Database(dataPath, {
    readOnly: config.readOnly ?? true,
  });

  return {
    async get(model: string, id: string): Promise<any> {
      return db.get(model, id);
    },

    async query(model: string, filters: Record<string, any>): Promise<any[]> {
      return db.list(model, filters);
    },

    async getRelations(model: string, id: string, relation: string): Promise<any[]> {
      return db.getRelations(model, id, relation);
    },

    close() {
      db.close();
    },
  };
}

/**
 * HTTP-based client (Sprint 17)
 */
function createHTTPClient(config: DBClientConfig): DBClient {
  const apiEndpoint = config.apiEndpoint || process.env.RUST_API_URL || "http://localhost:3000";

  return {
    async get(model: string, id: string): Promise<any> {
      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}/${id}`
      );

      if (!response.ok) {
        if (response.status === 404) {
          return null;
        }
        throw new Error(`HTTP error ${response.status}`);
      }

      return response.json();
    },

    async query(model: string, filters: Record<string, any>): Promise<any[]> {
      const queryParams = new URLSearchParams(filters as any);
      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}?${queryParams}`
      );

      if (!response.ok) {
        throw new Error(`HTTP error ${response.status}`);
      }

      return response.json();
    },

    async getRelations(model: string, id: string, relation: string): Promise<any[]> {
      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}/${id}/${relation}`
      );

      if (!response.ok) {
        return [];
      }

      return response.json();
    },
  };
}

// Export Database for direct use
export { Database } from "../ffi/Database";
export { QueryBuilder } from "../ffi/QueryBuilder";
```

**Tests**:
- FFI mode works
- HTTP mode works
- Auto-detection works
- Fallback works

**Success Criteria**:
- ✅ Backwards compatible with Sprint 17
- ✅ FFI mode works
- ✅ Clean abstraction
- ✅ Easy migration path

---

#### Task 4.2: Update Component Renderer
**Estimated**: 60 minutes
**Complexity**: Low
**Prerequisites**: Task 4.1

**Files to Modify**:
- `runtime/bun/src/server.ts`

**Changes**:

```typescript
// runtime/bun/src/server.ts

import { createDBClient } from "./db-client";

// Initialize DB client (auto-detects FFI vs HTTP)
const db = createDBClient({
  mode: "auto",
  dataPath: process.env.FORGEDB_DATA || "./data",
  readOnly: true,
});

// Component rendering (no changes needed - same API!)
async fetch(req: Request): Promise<Response> {
  const url = new URL(req.url);

  if (!url.pathname.startsWith("/components/")) {
    return new Response("Not Found", { status: 404 });
  }

  const parts = url.pathname.split("/").filter(Boolean);
  const [_, modelName, componentName, id] = parts;

  // Fetch data (now via FFI instead of HTTP!)
  const data = await db.get(modelName, id);

  if (!data) {
    return new Response("Not Found", { status: 404 });
  }

  // Fetch relations if needed
  if (component.relations) {
    for (const relation of component.relations) {
      data[relation] = await db.getRelations(modelName, id, relation);
    }
  }

  // Render component
  const Component = components[componentKey];
  const stream = await renderToReadableStream(
    <Component data={data} relations={data.relations} />
  );

  return new Response(stream, {
    headers: { "Content-Type": "text/html" },
  });
}
```

**Performance Comparison**:
```typescript
// Add performance logging
const start = performance.now();
const data = await db.get(modelName, id);
const duration = performance.now() - start;

console.log(`[Perf] get(${modelName}, ${id}): ${duration.toFixed(2)}ms`);
```

**Tests**:
- Component renders with FFI
- Relations fetched correctly
- Performance improvement verified
- Error handling works

**Success Criteria**:
- ✅ Components render correctly
- ✅ No API changes needed
- ✅ Performance improved
- ✅ Backwards compatible

---

#### Task 4.3: Update Route Handlers
**Estimated**: 90 minutes
**Complexity**: Medium
**Prerequisites**: Task 4.2

**Files to Modify**:
- `runtime/bun/src/server.ts` (route handler execution)
- Generated route handler stubs

**Changes**:

```typescript
// runtime/bun/src/server.ts

// Route handler execution
if (url.pathname.startsWith("/api/")) {
  // Parse route: /api/user/verify -> routes/user/verify/post.ts
  const parts = url.pathname.substring(5).split("/");
  const method = req.method.toLowerCase();

  try {
    // Dynamic import
    const handlerPath = `./routes/${parts.join("/")}/$ {method}.ts`;
    const handler = await import(handlerPath);

    // Call handler with request and DB client
    const response = await handler.default(req, db);
    return response;

  } catch (error) {
    console.error(`[Route] Error:`, error);
    return new Response("Internal Server Error", { status: 500 });
  }
}
```

**Update Handler Signature** (in codegen):
```typescript
// Generated: routes/user/verify/post.ts
import type { User } from '../../../generated/types';
import type { DBClient } from '../../../runtime/bun/src/db-client';

export default async function handler(
  req: Request,
  db: DBClient  // Now has direct DB access!
): Promise<Response> {
  try {
    const { userId, token } = await req.json();

    // Direct database access (FFI or HTTP depending on config)
    const user = await db.get('User', userId);

    if (!user) {
      return new Response(JSON.stringify({ error: 'User not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    // Verify token logic here...

    // For writes, still use Rust API
    await fetch('http://localhost:3000/api/users/${userId}', {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ verified: true }),
    });

    return new Response(JSON.stringify({ success: true }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });

  } catch (error) {
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
```

**Tests**:
- Route handler receives DB client
- Read operations via FFI
- Write operations via HTTP
- Error handling

**Success Criteria**:
- ✅ Handlers have DB access
- ✅ Type-safe DB client
- ✅ Reads via FFI
- ✅ Writes via HTTP

---

### Phase 5: Performance & Safety (Tasks 15-16)

**Estimated Total**: 4 hours
**Goal**: Validate performance and safety

---

#### Task 5.1: Performance Benchmarks
**Estimated**: 120 minutes
**Complexity**: Medium
**Prerequisites**: Phase 4 complete

**Files to Create**:
- `crates/ffi/benches/ffi_benchmark.rs`
- `runtime/bun/bench/ffi-vs-http.bench.ts`

**Rust Benchmark**:

```rust
// crates/ffi/benches/ffi_benchmark.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use forgedb_ffi::*;
use std::ffi::CString;
use std::ptr;
use tempfile::TempDir;

fn setup() -> (*mut ForgeDB, TempDir) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bench.db");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let db = forgedb_open(
        path_c.as_ptr(),
        FORGEDB_OPEN_CREATE,
        ptr::null_mut(),
    );

    // Insert test data
    // ...

    (db, temp)
}

fn bench_ffi_get(c: &mut Criterion) {
    let (db, _temp) = setup();

    let model = CString::new("User").unwrap();
    let id = CString::new("test-id").unwrap();

    c.bench_function("ffi_get", |b| {
        b.iter(|| {
            let result = forgedb_get(
                black_box(db),
                black_box(model.as_ptr()),
                black_box(id.as_ptr()),
                ptr::null_mut(),
            );
            if !result.is_null() {
                forgedb_free_string(result);
            }
        });
    });

    forgedb_close(db);
}

fn bench_ffi_list(c: &mut Criterion) {
    let (db, _temp) = setup();

    let model = CString::new("User").unwrap();

    for size in [10, 100, 1000].iter() {
        c.bench_with_input(
            BenchmarkId::new("ffi_list", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let result = forgedb_list(
                        black_box(db),
                        black_box(model.as_ptr()),
                        ptr::null(),
                        black_box(size),
                        0,
                        ptr::null_mut(),
                    );
                    if !result.is_null() {
                        forgedb_free_string(result);
                    }
                });
            },
        );
    }

    forgedb_close(db);
}

criterion_group!(benches, bench_ffi_get, bench_ffi_list);
criterion_main!(benches);
```

**Bun Benchmark**:

```typescript
// runtime/bun/bench/ffi-vs-http.bench.ts

import { bench, run } from "mitata";
import { Database } from "../ffi/Database";

// Setup
const db = new Database("./test_data", { readOnly: true });
const httpEndpoint = "http://localhost:3000";

bench("FFI: get single record", async () => {
  await db.get("User", "test-id");
});

bench("HTTP: get single record", async () => {
  await fetch(`${httpEndpoint}/api/users/test-id`).then(r => r.json());
});

bench("FFI: list 10 records", async () => {
  await db.list("User", {}, 10, 0);
});

bench("HTTP: list 10 records", async () => {
  await fetch(`${httpEndpoint}/api/users?limit=10`).then(r => r.json());
});

bench("FFI: list 100 records", async () => {
  await db.list("User", {}, 100, 0);
});

bench("HTTP: list 100 records", async () => {
  await fetch(`${httpEndpoint}/api/users?limit=100`).then(r => r.json());
});

bench("FFI: get with relations", async () => {
  const user = await db.get("User", "test-id");
  await db.getRelations("User", "test-id", "posts");
});

bench("HTTP: get with relations", async () => {
  await fetch(`${httpEndpoint}/api/users/test-id?include=posts`).then(r => r.json());
});

await run();

// Cleanup
db.close();
```

**Expected Results**:

| Operation | HTTP (Sprint 17) | FFI (Sprint 24) | Improvement |
|-----------|------------------|-----------------|-------------|
| Get single record | ~1-2ms | ~50-100μs | 10-20x |
| List 10 records | ~2-3ms | ~100-200μs | 10-15x |
| List 100 records | ~5-10ms | ~500μs-1ms | 5-10x |
| Get with relations | ~3-5ms | ~200-300μs | 10-15x |

**Success Criteria**:
- ✅ FFI is 10x faster than HTTP
- ✅ No performance regression
- ✅ Scalable with data size
- ✅ Results documented

---

#### Task 5.2: Memory Leak Detection & Testing
**Estimated**: 120 minutes
**Complexity**: High
**Prerequisites**: Phase 4 complete

**Files to Create**:
- `crates/ffi/tests/memory_test.rs`
- `runtime/bun/tests/memory-leak.test.ts`

**Rust Memory Tests**:

```rust
// crates/ffi/tests/memory_test.rs

use forgedb_ffi::*;
use std::ffi::CString;
use std::ptr;
use tempfile::TempDir;

#[test]
fn test_no_memory_leaks_get() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let db = forgedb_open(
        path_c.as_ptr(),
        FORGEDB_OPEN_CREATE,
        ptr::null_mut(),
    );

    let model = CString::new("User").unwrap();
    let id = CString::new("test-id").unwrap();

    // Run 10,000 times
    for _ in 0..10000 {
        let result = forgedb_get(
            db,
            model.as_ptr(),
            id.as_ptr(),
            ptr::null_mut(),
        );

        if !result.is_null() {
            forgedb_free_string(result);
        }
    }

    forgedb_close(db);
}

#[test]
fn test_no_memory_leaks_list() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let db = forgedb_open(
        path_c.as_ptr(),
        FORGEDB_OPEN_CREATE,
        ptr::null_mut(),
    );

    let model = CString::new("User").unwrap();

    for _ in 0..1000 {
        let result = forgedb_list(
            db,
            model.as_ptr(),
            ptr::null(),
            100,
            0,
            ptr::null_mut(),
        );

        if !result.is_null() {
            forgedb_free_string(result);
        }
    }

    forgedb_close(db);
}

#[test]
fn test_concurrent_access() {
    use std::thread;
    use std::sync::Arc;

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let db = forgedb_open(
        path_c.as_ptr(),
        FORGEDB_OPEN_CREATE,
        ptr::null_mut(),
    );

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let db = db;
            thread::spawn(move || {
                let model = CString::new("User").unwrap();
                let id = CString::new(format!("id-{}", i)).unwrap();

                for _ in 0..100 {
                    let result = forgedb_get(
                        db,
                        model.as_ptr(),
                        id.as_ptr(),
                        ptr::null_mut(),
                    );

                    if !result.is_null() {
                        forgedb_free_string(result);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    forgedb_close(db);
}
```

**Bun Memory Tests**:

```typescript
// runtime/bun/tests/memory-leak.test.ts

import { test, expect } from "bun:test";
import { Database } from "../ffi/Database";

test("no memory leaks - get operations", async () => {
  const db = new Database("./test_data", { readOnly: true });

  const initialMemory = process.memoryUsage().heapUsed;

  // Perform 10,000 operations
  for (let i = 0; i < 10000; i++) {
    await db.get("User", `id-${i % 100}`);

    // Force GC every 1000 operations
    if (i % 1000 === 0 && global.gc) {
      global.gc();
    }
  }

  // Force final GC
  if (global.gc) {
    global.gc();
  }

  const finalMemory = process.memoryUsage().heapUsed;
  const memoryGrowth = finalMemory - initialMemory;

  // Memory growth should be minimal (< 10MB)
  expect(memoryGrowth).toBeLessThan(10 * 1024 * 1024);

  db.close();
});

test("automatic cleanup on garbage collection", async () => {
  let db: Database | null = new Database("./test_data", { readOnly: true });

  // Use database
  await db.get("User", "test-id");

  // Remove reference (but don't call close())
  db = null;

  // Force GC
  if (global.gc) {
    global.gc();

    // Wait for finalization
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  // If we get here without crash, cleanup worked
  expect(true).toBe(true);
});

test("concurrent access safety", async () => {
  const db = new Database("./test_data", { readOnly: true });

  // Create 100 concurrent requests
  const promises = Array.from({ length: 100 }, (_, i) =>
    db.get("User", `id-${i % 10}`)
  );

  const results = await Promise.all(promises);

  // All requests should complete
  expect(results.length).toBe(100);

  db.close();
});
```

**Valgrind Testing** (optional, Linux only):

```bash
#!/bin/bash
# scripts/check-memory-leaks.sh

echo "Building FFI library..."
cargo build --release -p forgedb-ffi

echo "Running Valgrind..."
valgrind \
  --leak-check=full \
  --show-leak-kinds=all \
  --track-origins=yes \
  --verbose \
  --log-file=valgrind-out.txt \
  cargo test --release -p forgedb-ffi

echo "Checking results..."
if grep -q "ERROR SUMMARY: 0 errors" valgrind-out.txt; then
  echo "✅ No memory leaks detected"
  exit 0
else
  echo "❌ Memory leaks detected"
  cat valgrind-out.txt
  exit 1
fi
```

**Success Criteria**:
- ✅ No memory leaks in 10k operations
- ✅ Concurrent access safe
- ✅ Automatic cleanup works
- ✅ Valgrind clean (if applicable)

---

### Phase 6: Documentation & Examples (Tasks 17-18)

**Estimated Total**: 3 hours
**Goal**: Complete documentation

---

#### Task 6.1: Write FFI API Documentation
**Estimated**: 90 minutes
**Complexity**: Low
**Prerequisites**: All phases complete

**Files to Create**:
- `docs/FFI_API.md`
- `docs/MIGRATION_SPRINT17_TO_24.md`
- `runtime/bun/README.md`

**(Detailed documentation content provided in next message due to length)**

---

#### Task 6.2: Create Example Application
**Estimated**: 90 minutes
**Complexity**: Low
**Prerequisites**: Task 6.1

**Files to Create**:
- `examples/ffi-component-rendering/README.md`
- `examples/ffi-component-rendering/schema.forge`
- `examples/ffi-component-rendering/benchmark.ts`

**(Example application provided in next message)**

---

## Summary & Next Steps

**Total Implementation**:
- **18 tasks** across 6 phases
- **~26 hours** estimated (3-4 days focused work)
- **All dependencies mapped**
- **Complete test coverage planned**

**Key Milestones**:
1. Phase 1-2: FFI bridge working (basic get/list)
2. Phase 3: TypeScript bindings complete
3. Phase 4: Sprint 17 integration complete
4. Phase 5: Performance validated (10x improvement)
5. Phase 6: Documentation complete

**Ready to proceed with**:
- Immediate start on Phase 1, Task 1.1
- Parallel work possible after Phase 1 complete
- Clear success criteria for each task

Would you like me to:
1. **Start implementing Phase 1** (FFI Bridge Architecture)?
2. **Create the example application** first to clarify requirements?
3. **Write the documentation templates** to guide implementation?
4. **Set up the build system** and CI/CD for the FFI crate?
