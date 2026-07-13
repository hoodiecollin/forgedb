//! No-op single-writer lock for the browser target.
//!
//! The native `DirLock` uses an OS advisory file lock to enforce
//! single-writer-per-process. In a browser tab there is exactly one follower
//! instance touching the arena store (`wasm32` is single-threaded), so the lock
//! is trivially held. This stub matches the native `DirLock::acquire` signature
//! so the generated `Database::open_at` links unchanged; it always succeeds.

use std::io;
use std::path::Path;

/// A no-op single-writer lock. Always acquired; releasing it is a no-op.
pub struct DirLock;

impl std::fmt::Debug for DirLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirLock").finish_non_exhaustive()
    }
}

impl DirLock {
    /// Always returns `Ok(DirLock)` — there is no second writer to exclude in a
    /// single-threaded browser follower.
    pub fn acquire(_root: &Path) -> io::Result<DirLock> {
        Ok(DirLock)
    }
}
