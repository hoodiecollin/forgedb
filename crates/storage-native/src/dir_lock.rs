use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;

pub const LOCK_FILENAME: &str = ".forgedb.lock";

pub struct DirLock {
    _file: std::fs::File,
}

impl std::fmt::Debug for DirLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirLock").finish_non_exhaustive()
    }
}

impl DirLock {
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

#[cfg(unix)]
fn libc_ewouldblock() -> i32 {
    libc::EWOULDBLOCK
}

#[cfg(not(unix))]
fn libc_ewouldblock() -> i32 {
    i32::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_filename_is_stable() {
        assert_eq!(LOCK_FILENAME, ".forgedb.lock");
    }
}
