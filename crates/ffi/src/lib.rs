//! ForgeDB FFI Bindings
//!
//! C-compatible FFI bindings for ForgeDB, enabling direct database access from
//! Bun, Node.js, and other runtimes.
//!
//! # Overview
//!
//! This crate provides a C-compatible Foreign Function Interface (FFI) for ForgeDB,
//! allowing integration with:
//!
//! - **Bun** - Direct FFI calls using Bun's native FFI support
//! - **Node.js** - Through native addons or ffi-napi
//! - **Python** - Via ctypes or cffi
//! - **Other languages** - Any language with C FFI support
//!
//! # Architecture
//!
//! The FFI layer provides:
//!
//! - **Opaque handles** - Safe pointer management for Rust objects
//! - **C-compatible types** - All types are C-ABI compatible
//! - **Error handling** - Error codes and message passing
//! - **Memory safety** - Explicit allocation/deallocation functions
//!
//! ## Safety Guarantees
//!
//! - All `unsafe` operations are properly documented
//! - Null pointer checks on all public APIs
//! - Opaque handles prevent direct memory access
//! - Explicit memory management (no hidden allocations)
//!
//! # Status
//!
//! This crate is currently in early development. Most functionality is planned
//! but not yet implemented.
//!
//! ## Currently Available
//!
//! - `forgedb_version()` - Get ForgeDB version string
//!
//! ## Planned
//!
//! - Database open/close operations
//! - CRUD operations
//! - Query interface
//! - Transaction support
//! - Error handling infrastructure
//!
//! # Examples
//!
//! ## C Usage
//!
//! ```c
//! #include <forgedb.h>
//! #include <stdio.h>
//!
//! int main() {
//!     // Get version
//!     const char* version = forgedb_version();
//!     printf("ForgeDB version: %s\n", version);
//!     return 0;
//! }
//! ```
//!
//! ## Bun FFI Usage
//!
//! ```typescript
//! import { dlopen, FFIType, suffix } from "bun:ffi";
//!
//! const lib = dlopen(`libforgedb.${suffix}`, {
//!   forgedb_version: {
//!     returns: FFIType.cstring,
//!   },
//! });
//!
//! // Get version
//! console.log(lib.symbols.forgedb_version());
//! ```
//!
//! ## Python Usage (ctypes)
//!
//! ```python
//! from ctypes import *
//!
//! # Load library
//! lib = CDLL("./libforgedb.so")
//!
//! # Define function signatures
//! lib.forgedb_version.restype = c_char_p
//!
//! # Get version
//! version = lib.forgedb_version()
//! print(f"ForgeDB version: {version.decode('utf-8')}")
//! ```
//!
//! # Building
//!
//! To build the FFI library:
//!
//! ```bash
//! cargo build --release -p forgedb-ffi
//! ```
//!
//! This produces:
//! - **Linux**: `libforgedb.so`
//! - **macOS**: `libforgedb.dylib`
//! - **Windows**: `forgedb.dll`

use std::os::raw::c_char;

/// Get ForgeDB version string
///
/// Returns a static string with the version number.
/// No need to free (static storage).
///
/// # Safety
///
/// This function is safe to call from any thread. The returned pointer
/// points to static storage and must not be freed.
///
/// # Returns
///
/// Pointer to null-terminated version string (e.g., "0.1.0")
#[unsafe(no_mangle)]
pub extern "C" fn forgedb_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn test_version() {
        let version_ptr = forgedb_version();
        assert!(!version_ptr.is_null());

        let version = unsafe { CStr::from_ptr(version_ptr) };
        let version_str = version.to_str().unwrap();

        // Should be a valid semver-ish string
        assert!(!version_str.is_empty());
        assert!(version_str.contains('.'));
    }
}
