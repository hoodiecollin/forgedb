use forgedb_ffi::*;
use std::sync::Arc;

#[test]
fn test_insert_and_get() {
    let registry = HandleRegistry::<i32>::new();

    let value = 42;
    let handle = registry.insert(value);

    let retrieved = registry.get(handle);
    assert!(retrieved.is_some());
    assert_eq!(*retrieved.unwrap(), 42);
}

#[test]
fn test_remove() {
    let registry = HandleRegistry::<i32>::new();

    let handle = registry.insert(100);
    assert!(registry.get(handle).is_some());

    registry.remove(handle);
    assert!(registry.get(handle).is_none());
}

#[test]
fn test_null_handle() {
    let registry = HandleRegistry::<i32>::new();

    let null_handle: *mut i32 = std::ptr::null_mut();
    assert!(registry.get(null_handle).is_none());

    // Should not panic
    registry.remove(null_handle);
}

#[test]
fn test_concurrent_access() {
    use std::thread;

    let registry = Arc::new(HandleRegistry::<String>::new());

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let r = Arc::clone(&registry);
            thread::spawn(move || {
                let handle = r.insert(format!("value{}", i));
                // Convert to usize to safely send across thread
                handle as usize
            })
        })
        .collect();

    let handle_ids: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for (i, handle_id) in handle_ids.iter().enumerate() {
        // Convert back from usize to pointer
        let handle = *handle_id as *mut String;
        let value = registry.get(handle);
        assert!(value.is_some());
        assert_eq!(*value.unwrap(), format!("value{}", i));
    }
}

#[test]
fn test_multiple_handles() {
    let registry = HandleRegistry::<String>::new();

    let h1 = registry.insert("first".to_string());
    let h2 = registry.insert("second".to_string());
    let h3 = registry.insert("third".to_string());

    assert_eq!(*registry.get(h1).unwrap(), "first");
    assert_eq!(*registry.get(h2).unwrap(), "second");
    assert_eq!(*registry.get(h3).unwrap(), "third");

    registry.remove(h2);

    assert_eq!(*registry.get(h1).unwrap(), "first");
    assert!(registry.get(h2).is_none());
    assert_eq!(*registry.get(h3).unwrap(), "third");
}

#[test]
fn test_handle_uniqueness() {
    let registry = HandleRegistry::<i32>::new();

    let h1 = registry.insert(1);
    let h2 = registry.insert(2);

    assert_ne!(h1, h2);
}
