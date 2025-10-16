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
    let handle = ErrorHandle {
        code,
        message: format!("{}\0", message),
    };
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
                let error_string = e.to_string();
                let (code, msg) = if error_string.contains("not found")
                    || error_string.contains("Not found")
                    || error_string.contains("does not exist")
                {
                    ($crate::errors::FORGEDB_ERR_NOT_FOUND, error_string)
                } else if error_string.contains("IO")
                    || error_string.contains("I/O")
                    || error_string.contains("file")
                    || error_string.contains("permission")
                {
                    ($crate::errors::FORGEDB_ERR_IO, error_string)
                } else if error_string.contains("invalid")
                    || error_string.contains("Invalid")
                    || error_string.contains("malformed")
                {
                    ($crate::errors::FORGEDB_ERR_INVALID, error_string)
                } else {
                    ($crate::errors::FORGEDB_ERR_INTERNAL, error_string)
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
    use std::ffi::CStr;

    #[test]
    fn test_create_error() {
        let err = create_error(FORGEDB_ERR_IO, "Test error".to_string());
        assert!(!err.is_null());

        let code = forgedb_error_code(err);
        assert_eq!(code, FORGEDB_ERR_IO);

        let msg = unsafe { CStr::from_ptr(forgedb_error_message(err)) };
        assert_eq!(msg.to_str().unwrap(), "Test error");

        forgedb_free_error(err);
    }

    #[test]
    fn test_null_error() {
        let code = forgedb_error_code(ptr::null_mut());
        assert_eq!(code, FORGEDB_ERR_INVALID);

        let msg = unsafe { CStr::from_ptr(forgedb_error_message(ptr::null_mut())) };
        assert_eq!(msg.to_str().unwrap(), "Invalid error handle");

        // Should not crash
        forgedb_free_error(ptr::null_mut());
    }

    #[test]
    fn test_set_error() {
        let mut err_ptr: *mut ForgeDBError = ptr::null_mut();

        set_error(&mut err_ptr, FORGEDB_ERR_NOT_FOUND, "Item not found".to_string());

        assert!(!err_ptr.is_null());
        assert_eq!(forgedb_error_code(err_ptr), FORGEDB_ERR_NOT_FOUND);

        let msg = unsafe { CStr::from_ptr(forgedb_error_message(err_ptr)) };
        assert_eq!(msg.to_str().unwrap(), "Item not found");

        forgedb_free_error(err_ptr);
    }

    #[test]
    fn test_set_error_null_output() {
        // Should not crash when error_out is null
        set_error(ptr::null_mut(), FORGEDB_ERR_IO, "Test".to_string());
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(FORGEDB_OK, 0);
        assert_eq!(FORGEDB_ERR_IO, 1);
        assert_eq!(FORGEDB_ERR_NOT_FOUND, 2);
        assert_eq!(FORGEDB_ERR_INVALID, 3);
        assert_eq!(FORGEDB_ERR_INTERNAL, 4);
    }

    #[test]
    fn test_multiple_errors() {
        let err1 = create_error(FORGEDB_ERR_IO, "Error 1".to_string());
        let err2 = create_error(FORGEDB_ERR_NOT_FOUND, "Error 2".to_string());
        let err3 = create_error(FORGEDB_ERR_INVALID, "Error 3".to_string());

        assert_eq!(forgedb_error_code(err1), FORGEDB_ERR_IO);
        assert_eq!(forgedb_error_code(err2), FORGEDB_ERR_NOT_FOUND);
        assert_eq!(forgedb_error_code(err3), FORGEDB_ERR_INVALID);

        let msg1 = unsafe { CStr::from_ptr(forgedb_error_message(err1)) };
        let msg2 = unsafe { CStr::from_ptr(forgedb_error_message(err2)) };
        let msg3 = unsafe { CStr::from_ptr(forgedb_error_message(err3)) };

        assert_eq!(msg1.to_str().unwrap(), "Error 1");
        assert_eq!(msg2.to_str().unwrap(), "Error 2");
        assert_eq!(msg3.to_str().unwrap(), "Error 3");

        forgedb_free_error(err1);
        forgedb_free_error(err2);
        forgedb_free_error(err3);
    }
}
