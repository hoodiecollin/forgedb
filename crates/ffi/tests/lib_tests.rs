use forgedb_ffi::*;
use std::ffi::{CStr, CString};
use std::ptr;
use tempfile::TempDir;

fn setup_test_db() -> (*mut ForgeDB, TempDir) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let db = forgedb_open(path_c.as_ptr(), FORGEDB_OPEN_CREATE, ptr::null_mut());

    assert!(!db.is_null());

    (db, temp)
}

#[test]
fn test_version() {
    let version = unsafe { CStr::from_ptr(forgedb_version()) };
    assert!(!version.to_bytes().is_empty());
    assert!(version.to_str().unwrap().starts_with("0.1."));
}

#[test]
fn test_open_close() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test.db");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let mut err: *mut ForgeDBError = ptr::null_mut();
    let db = forgedb_open(path_c.as_ptr(), FORGEDB_OPEN_CREATE, &mut err);

    assert!(!db.is_null());
    assert!(err.is_null());

    forgedb_close(db);
}

#[test]
fn test_open_nonexistent() {
    let path_c = CString::new("/nonexistent/path/db").unwrap();

    let mut err: *mut ForgeDBError = ptr::null_mut();
    let db = forgedb_open(path_c.as_ptr(), FORGEDB_OPEN_READONLY, &mut err);

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
    let (db, _temp) = setup_test_db();

    forgedb_close(db);
    forgedb_close(db); // Should be safe
}

#[test]
fn test_list_empty() {
    let (db, _temp) = setup_test_db();

    let model = CString::new("User").unwrap();

    let mut err: *mut ForgeDBError = ptr::null_mut();
    let json = forgedb_list(db, model.as_ptr(), ptr::null(), 0, 0, &mut err);

    assert!(!json.is_null());
    assert!(err.is_null());

    let json_str = unsafe { CStr::from_ptr(json).to_str().unwrap() };
    assert_eq!(json_str, "[]");

    forgedb_free_string(json);
    forgedb_close(db);
}

#[test]
fn test_get_invalid_handle() {
    let model = CString::new("User").unwrap();
    let id = CString::new("123").unwrap();

    let mut err: *mut ForgeDBError = ptr::null_mut();
    let json = forgedb_get(ptr::null_mut(), model.as_ptr(), id.as_ptr(), &mut err);

    assert!(json.is_null());
    assert!(!err.is_null());

    let code = forgedb_error_code(err);
    assert_eq!(code, FORGEDB_ERR_INVALID);

    forgedb_free_error(err);
}
