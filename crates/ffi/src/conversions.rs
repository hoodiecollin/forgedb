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

