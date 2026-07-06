//! ForgeDB FFI Bindings
//!
//! C-compatible FFI bindings for ForgeDB, intended for direct database access from
//! Bun, Node.js, and other runtimes. This crate is in early development — only a
//! single function is currently exported; the full API is planned but not yet built.
//!
//! # Currently Available
//!
//! - [`forgedb_version`] — returns the crate version as a static C string.
//!
//! # Planned (not yet implemented)
//!
//! - Opaque database handles (open/close)
//! - CRUD operations
//! - Query interface
//! - Transaction support
//! - Error-code infrastructure
//!
//! # Building
//!
//! ```bash
//! cargo build --release -p forgedb-ffi
//! ```
//!
//! This produces:
//! - **Linux**: `libforgedb.so`
//! - **macOS**: `libforgedb.dylib`
//! - **Windows**: `forgedb.dll`
//!
//! # Example (C)
//!
//! ```c
//! #include <stdio.h>
//! // extern declaration — no forgedb.h yet
//! extern const char* forgedb_version(void);
//!
//! int main() {
//!     printf("ForgeDB version: %s\n", forgedb_version());
//!     return 0;
//! }
//! ```
//!
//! # Example (Bun FFI)
//!
//! ```typescript
//! import { dlopen, FFIType, suffix } from "bun:ffi";
//!
//! const lib = dlopen(`libforgedb.${suffix}`, {
//!   forgedb_version: { returns: FFIType.cstring },
//! });
//!
//! console.log(lib.symbols.forgedb_version());
//! ```

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
