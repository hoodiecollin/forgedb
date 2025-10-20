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

use libc::{c_char, c_int};
use std::path::PathBuf;
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

// Flags
pub const FORGEDB_OPEN_READONLY: c_int = 0x01;
pub const FORGEDB_OPEN_CREATE: c_int = 0x02;

/// Get ForgeDB version string
///
/// Returns a static string with the version number.
/// No need to free (static storage).
#[no_mangle]
pub extern "C" fn forgedb_version() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

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
            set_error(
                error,
                FORGEDB_ERR_INVALID,
                "Invalid path".to_string(),
            );
            return ptr::null_mut();
        }
    };

    // Parse flags (currently only READONLY is meaningful for our use case)
    let _readonly = (flags & FORGEDB_OPEN_READONLY) != 0;
    let _create = (flags & FORGEDB_OPEN_CREATE) != 0;

    // Open database
    let storage = ffi_try!(
        forgedb_storage::UserStorage::new(PathBuf::from(&path_str)),
        error
    );

    // Create handle
    let handle = DatabaseHandle {
        storage: std::sync::Arc::new(parking_lot::RwLock::new(storage)),
        path: path_str,
    };

    DB_HANDLES.insert(handle) as *mut ForgeDB
}

/// Close a ForgeDB database
///
/// After this call, the handle is invalid and must not be used.
/// Safe to call with NULL or already-closed handle.
#[no_mangle]
pub extern "C" fn forgedb_close(db: *mut ForgeDB) {
    if !db.is_null() {
        DB_HANDLES.remove(db as *mut DatabaseHandle);
    }
}

/// Get a single record by ID
///
/// # Parameters
/// - `db`: Database handle
/// - `model`: Model name (currently ignored, only "User" supported)
/// - `id`: Record ID as string
/// - `error`: Output parameter for error (can be NULL)
///
/// # Returns
/// - JSON string on success (must be freed with forgedb_free_string)
/// - NULL on error or not found
#[no_mangle]
pub extern "C" fn forgedb_get(
    db: *mut ForgeDB,
    _model: *const c_char,
    id: *const c_char,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    let db_handle = match DB_HANDLES.get(db as *mut DatabaseHandle) {
        Some(h) => h,
        None => {
            set_error(
                error,
                FORGEDB_ERR_INVALID,
                "Invalid database handle".to_string(),
            );
            return ptr::null_mut();
        }
    };

    // Convert ID parameter
    let id_str = match c_str_to_rust(id) {
        Some(s) => s,
        None => {
            set_error(
                error,
                FORGEDB_ERR_INVALID,
                "Invalid id".to_string(),
            );
            return ptr::null_mut();
        }
    };

    // Parse ID as u64
    let id_u64 = match id_str.parse::<u64>() {
        Ok(id) => id,
        Err(_) => {
            set_error(
                error,
                FORGEDB_ERR_INVALID,
                format!("Invalid id format: {}", id_str),
            );
            return ptr::null_mut();
        }
    };

    // Get from database
    let mut storage = db_handle.storage.write();
    let result = ffi_try!(storage.get(id_u64), error);

    match result {
        Some(user) => to_json_string(&user),
        None => {
            set_error(
                error,
                FORGEDB_ERR_NOT_FOUND,
                format!("Record not found: {}", id_str),
            );
            ptr::null_mut()
        }
    }
}

/// List records with optional filtering
///
/// # Parameters
/// - `db`: Database handle
/// - `model`: Model name (currently ignored)
/// - `filter_json`: JSON object with filters (currently ignored, returns all)
/// - `limit`: Maximum number of records (0 for all)
/// - `offset`: Number of records to skip (0 for none)
/// - `error`: Output parameter for error
///
/// # Returns
/// - JSON array string on success (must be freed)
/// - NULL on error
#[no_mangle]
pub extern "C" fn forgedb_list(
    db: *mut ForgeDB,
    _model: *const c_char,
    _filter_json: *const c_char,
    limit: i32,
    offset: i32,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    let db_handle = match DB_HANDLES.get(db as *mut DatabaseHandle) {
        Some(h) => h,
        None => {
            set_error(
                error,
                FORGEDB_ERR_INVALID,
                "Invalid database handle".to_string(),
            );
            return ptr::null_mut();
        }
    };

    // Get all users
    let mut storage = db_handle.storage.write();
    let all_users = ffi_try!(storage.list_all(), error);

    // Apply pagination
    let offset_usize = if offset > 0 { offset as usize } else { 0 };
    let results: Vec<_> = all_users
        .into_iter()
        .skip(offset_usize)
        .take(if limit > 0 {
            limit as usize
        } else {
            usize::MAX
        })
        .collect();

    // Serialize to JSON array
    to_json_string(&results)
}

/// Execute complex query (simplified version)
///
/// Currently just delegates to list with limit/offset from query JSON
#[no_mangle]
pub extern "C" fn forgedb_query(
    db: *mut ForgeDB,
    model: *const c_char,
    query_json: *const c_char,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Parse query JSON for limit/offset
    #[derive(serde::Deserialize)]
    struct Query {
        #[serde(default)]
        limit: Option<i32>,
        #[serde(default)]
        offset: Option<i32>,
    }

    let query: Query = match from_json_string(query_json) {
        Some(q) => q,
        None => {
            set_error(
                error,
                FORGEDB_ERR_INVALID,
                "Invalid query JSON".to_string(),
            );
            return ptr::null_mut();
        }
    };

    // Delegate to list
    forgedb_list(
        db,
        model,
        ptr::null(),
        query.limit.unwrap_or(0),
        query.offset.unwrap_or(0),
        error,
    )
}

/// Get related records (not implemented yet for simple User model)
///
/// Returns empty array for now
#[no_mangle]
pub extern "C" fn forgedb_get_relations(
    db: *mut ForgeDB,
    _model: *const c_char,
    _id: *const c_char,
    _relation_name: *const c_char,
    error: *mut *mut ForgeDBError,
) -> *mut c_char {
    // Validate handle
    if DB_HANDLES.get(db as *mut DatabaseHandle).is_none() {
        set_error(
            error,
            FORGEDB_ERR_INVALID,
            "Invalid database handle".to_string(),
        );
        return ptr::null_mut();
    }

    // Return empty array (no relations in simple User model)
    let empty: Vec<serde_json::Value> = vec![];
    to_json_string(&empty)
}
