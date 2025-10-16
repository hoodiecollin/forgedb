//! Handle management for FFI
//!
//! Provides safe handle-based access to Rust objects across FFI boundary.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Thread-safe registry for managing opaque handles
pub struct HandleRegistry<T> {
    next_id: AtomicUsize,
    handles: Arc<RwLock<HashMap<usize, Arc<T>>>>,
}

impl<T> HandleRegistry<T> {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            next_id: AtomicUsize::new(1), // Start at 1, reserve 0 for NULL
            handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a value and return an opaque handle
    ///
    /// Returns a pointer that's actually just an ID cast to pointer.
    /// The actual data stays in Rust.
    pub fn insert(&self, value: T) -> *mut T {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let arc = Arc::new(value);

        self.handles.write().insert(id, arc);

        id as *mut T
    }

    /// Get a value by handle
    ///
    /// Returns None if handle is invalid or has been removed.
    pub fn get(&self, handle: *mut T) -> Option<Arc<T>> {
        if handle.is_null() {
            return None;
        }

        let id = handle as usize;
        self.handles.read().get(&id).cloned()
    }

    /// Remove and drop a handle
    ///
    /// After this call, the handle is invalid.
    /// Safe to call multiple times (subsequent calls are no-op).
    pub fn remove(&self, handle: *mut T) -> bool {
        if handle.is_null() {
            return false;
        }

        let id = handle as usize;
        self.handles.write().remove(&id).is_some()
    }

    /// Check if a handle is valid
    pub fn is_valid(&self, handle: *mut T) -> bool {
        if handle.is_null() {
            return false;
        }

        let id = handle as usize;
        self.handles.read().contains_key(&id)
    }

    /// Get the number of active handles
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.handles.read().len()
    }
}

impl<T> Default for HandleRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Global registries
lazy_static::lazy_static! {
    pub static ref DB_HANDLES: HandleRegistry<DatabaseHandle> = HandleRegistry::new();
    pub static ref ERROR_HANDLES: HandleRegistry<ErrorHandle> = HandleRegistry::new();
}

/// Internal database handle (never exposed directly)
pub struct DatabaseHandle {
    pub storage: Arc<RwLock<forgedb_storage::UserStorage>>,
    pub path: String,
}

/// Internal error handle (never exposed directly)
pub struct ErrorHandle {
    pub code: i32,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let registry = HandleRegistry::<i32>::new();

        let handle = registry.insert(42);
        assert!(!handle.is_null());

        let value = registry.get(handle).unwrap();
        assert_eq!(*value, 42);
    }

    #[test]
    fn test_remove() {
        let registry = HandleRegistry::<i32>::new();

        let handle = registry.insert(42);
        assert!(registry.is_valid(handle));

        assert!(registry.remove(handle));
        assert!(!registry.is_valid(handle));

        // Second remove returns false
        assert!(!registry.remove(handle));
    }

    #[test]
    fn test_null_handle() {
        let registry = HandleRegistry::<i32>::new();

        assert!(registry.get(std::ptr::null_mut()).is_none());
        assert!(!registry.is_valid(std::ptr::null_mut()));
        assert!(!registry.remove(std::ptr::null_mut()));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let registry = Arc::new(HandleRegistry::<i32>::new());
        let handle = registry.insert(42);

        // Convert to usize for Send-ability
        let handle_id = handle as usize;

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let registry = registry.clone();
                thread::spawn(move || {
                    let handle = handle_id as *mut i32;
                    for _ in 0..100 {
                        let value = registry.get(handle).unwrap();
                        assert_eq!(*value, 42);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_multiple_handles() {
        let registry = HandleRegistry::<String>::new();

        let h1 = registry.insert("first".to_string());
        let h2 = registry.insert("second".to_string());
        let h3 = registry.insert("third".to_string());

        assert_eq!(registry.len(), 3);

        assert_eq!(*registry.get(h1).unwrap(), "first");
        assert_eq!(*registry.get(h2).unwrap(), "second");
        assert_eq!(*registry.get(h3).unwrap(), "third");

        registry.remove(h2);
        assert_eq!(registry.len(), 2);

        assert!(registry.get(h2).is_none());
        assert!(registry.get(h1).is_some());
        assert!(registry.get(h3).is_some());
    }

    #[test]
    fn test_handle_uniqueness() {
        let registry = HandleRegistry::<i32>::new();

        let h1 = registry.insert(1);
        let h2 = registry.insert(2);
        let h3 = registry.insert(3);

        // Handles should be unique
        assert_ne!(h1 as usize, h2 as usize);
        assert_ne!(h2 as usize, h3 as usize);
        assert_ne!(h1 as usize, h3 as usize);

        // All handles should be valid
        assert!(registry.is_valid(h1));
        assert!(registry.is_valid(h2));
        assert!(registry.is_valid(h3));
    }
}
