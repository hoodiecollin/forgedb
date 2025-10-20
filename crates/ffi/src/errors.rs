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
