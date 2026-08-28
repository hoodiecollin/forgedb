use std::io;
use std::path::Path;

pub struct DirLock;

impl std::fmt::Debug for DirLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirLock").finish_non_exhaustive()
    }
}

impl DirLock {
    pub fn acquire(_root: &Path) -> io::Result<DirLock> {
        Ok(DirLock)
    }
}
