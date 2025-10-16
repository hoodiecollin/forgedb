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

        let result = unsafe { CStr::from_ptr(c_str).to_str().unwrap() };
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

    #[test]
    fn test_empty_string() {
        let rust_str = "".to_string();
        let c_str = rust_str_to_c(rust_str);
        assert!(!c_str.is_null());

        let result = unsafe { CStr::from_ptr(c_str).to_str().unwrap() };
        assert_eq!(result, "");

        forgedb_free_string(c_str);
    }

    #[test]
    fn test_unicode() {
        let rust_str = "Hello 世界 🦀".to_string();
        let c_str = rust_str_to_c(rust_str.clone());
        assert!(!c_str.is_null());

        let result = unsafe { CStr::from_ptr(c_str).to_str().unwrap() };
        assert_eq!(result, rust_str);

        forgedb_free_string(c_str);
    }

    #[test]
    fn test_json_array() {
        let vec = vec![1, 2, 3, 4, 5];
        let json_str = to_json_string(&vec);
        assert!(!json_str.is_null());

        let result: Vec<i32> = from_json_string(json_str).unwrap();
        assert_eq!(result, vec);

        forgedb_free_string(json_str);
    }

    #[test]
    fn test_json_object() {
        use std::collections::HashMap;

        let mut map = HashMap::new();
        map.insert("key1", "value1");
        map.insert("key2", "value2");

        let json_str = to_json_string(&map);
        assert!(!json_str.is_null());

        let result: HashMap<String, String> = from_json_string(json_str).unwrap();
        assert_eq!(result.get("key1").unwrap(), "value1");
        assert_eq!(result.get("key2").unwrap(), "value2");

        forgedb_free_string(json_str);
    }

    #[test]
    fn test_invalid_json() {
        let c_str = CString::new("{invalid json}").unwrap();
        let result: Option<serde_json::Value> = from_json_string(c_str.as_ptr());
        assert!(result.is_none());
    }

    #[test]
    fn test_string_with_null_byte_fails() {
        let rust_str = "hello\0world".to_string();
        let c_str = rust_str_to_c(rust_str);
        // Should return null because CString cannot contain null bytes
        assert!(c_str.is_null());
    }
}
