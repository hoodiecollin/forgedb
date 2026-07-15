//! Advisory single-writer lock for a ForgeDB data directory.
//!
//! Enforces the v1 single-writer-per-process contract at the OS advisory-lock
//! level: the first process to call [`DirLock::acquire`] on a data directory
//! holds an exclusive lock; any subsequent attempt — from a different process —
//! returns `Err` with `ErrorKind::WouldBlock` so the caller can print a clear
//! diagnostic and exit cleanly.
//!
//! The lock is **not** a lease, a registry, or a distributed coordinator — it
//! only prevents two writers from accidentally opening the same directory.
//! Concurrent-writer serialization is explicitly out of scope for v1.
//!
//! The lock file is `<root>/.forgedb.lock`.  It is created if absent and is
//! never deleted on drop — the OS reclaims the advisory lock automatically when
//! the file handle closes.

use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;

/// The single-writer lock filename under a data directory (`<root>/.forgedb.lock`).
///
/// **Cross-crate contract:** `forgedb-coordinator` advisory-locks this same file
/// (by a duplicated constant, since it must not depend on `forgedb-storage*` —
/// MVCC spec T3-8) so a coordinator and a standalone writer are mutually
/// exclusive (T3-5). Renaming it here without updating
/// `forgedb_coordinator::server::DIR_LOCK_FILENAME` breaks that exclusion; a
/// parity test in each crate pins the string.
pub const LOCK_FILENAME: &str = ".forgedb.lock";

/// An advisory exclusive lock on a ForgeDB data directory.
///
/// Holds an open `File` to `<root>/.forgedb.lock` with an OS-level exclusive
/// advisory lock (via [`fs2::FileExt::try_lock_exclusive`]).  The lock is
/// released automatically when this value is dropped (the `File` closes, which
/// the OS uses to release the advisory lock).
///
/// # Acquiring
///
/// ```no_run
/// use forgedb_storage_native::DirLock;
/// use std::path::Path;
///
/// let lock = DirLock::acquire(Path::new("./data"))?;
/// // lock is held for the lifetime of the value
/// drop(lock); // released here
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Conflicts
///
/// If another process already holds the lock, [`acquire`](DirLock::acquire)
/// returns `Err` with `kind() == io::ErrorKind::WouldBlock`.  The caller
/// should print a human-readable message and exit:
///
/// ```no_run
/// use forgedb_storage_native::DirLock;
/// use std::path::Path;
///
/// match DirLock::acquire(Path::new("./data")) {
///     Ok(lock) => { /* proceed */ }
///     Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
///         eprintln!("error: another ForgeDB writer already has this data directory open");
///         std::process::exit(1);
///     }
///     Err(e) => return Err(e),
/// }
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct DirLock {
    // The File holds the advisory lock; closing it releases the lock.
    // No explicit unlock is needed: fs2 releases on drop via the OS.
    _file: std::fs::File,
}

impl std::fmt::Debug for DirLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirLock").finish_non_exhaustive()
    }
}

impl DirLock {
    /// Try to acquire an exclusive advisory lock on `<root>/.forgedb.lock`,
    /// creating the file (and any missing parent directories) if needed.
    ///
    /// Returns `Ok(DirLock)` on success.  If another process already holds the
    /// lock, returns `Err` with `kind() == io::ErrorKind::WouldBlock` so the
    /// caller can print a clear "another writer already has this data dir open"
    /// message and exit.
    ///
    /// # Errors
    ///
    /// - `WouldBlock` — another process already holds the exclusive lock.
    /// - Any other `io::Error` from creating the directory or opening the file.
    pub fn acquire(root: &Path) -> io::Result<DirLock> {
        fs::create_dir_all(root)?;
        let lock_path = root.join(LOCK_FILENAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        file.try_lock_exclusive().map_err(|e| {
            // fs2 surfaces a lock-held conflict as a platform-specific OS error.
            // Normalise it to WouldBlock so callers don't need to match on
            // platform codes.
            if e.kind() == io::ErrorKind::WouldBlock
                || e.raw_os_error() == Some(libc_ewouldblock())
            {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another ForgeDB writer already has this data directory open",
                )
            } else {
                e
            }
        })?;

        Ok(DirLock { _file: file })
    }
}

/// The platform EWOULDBLOCK code, used to normalise fs2 lock-conflict errors.
///
/// On most POSIX systems EWOULDBLOCK == EAGAIN (11 on Linux, 35 on macOS).
/// We only need this as a fallback for older fs2 versions that may not surface
/// the error as `WouldBlock` directly.
#[cfg(unix)]
fn libc_ewouldblock() -> i32 {
    // SAFETY: `EWOULDBLOCK` is a compile-time constant; no unsafe memory ops.
    libc::EWOULDBLOCK
}

#[cfg(not(unix))]
fn libc_ewouldblock() -> i32 {
    // On non-Unix targets fs2 should already set the kind to WouldBlock.
    // Return an impossible value so the fallback branch is never taken.
    i32::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    /// G3 mirror (PM re-gate #84) — pins the lock filename so the coordinator's
    /// duplicated `DIR_LOCK_FILENAME` constant cannot silently desync from the
    /// file `DirLock` actually locks. The other half lives in
    /// `forgedb_coordinator::server::tests::lock_filename_matches_storage_dirlock`.
    #[test]
    fn lock_filename_is_stable() {
        assert_eq!(LOCK_FILENAME, ".forgedb.lock");
    }
}
