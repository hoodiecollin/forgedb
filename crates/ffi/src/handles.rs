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
