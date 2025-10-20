use forgedb_ffi::*;
use std::ffi::CStr;
use std::ptr;

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
