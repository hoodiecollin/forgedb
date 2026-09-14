An advisory exclusive lock on a ForgeDB data directory.

Holds an open `File` to `<root>/.forgedb.lock` with an OS-level exclusive
advisory lock (via [`fs2::FileExt::try_lock_exclusive`]).  The lock is
released automatically when this value is dropped (the `File` closes, which
the OS uses to release the advisory lock).

# Acquiring

```no_run
use forgedb_storage_native::DirLock;
use std::path::Path;

let lock = DirLock::acquire(Path::new("./data"))?;
// lock is held for the lifetime of the value
drop(lock); // released here
# Ok::<(), std::io::Error>(())
```

# Conflicts

If another process already holds the lock, [`acquire`](DirLock::acquire)
returns `Err` with `kind() == io::ErrorKind::WouldBlock`.  The caller
should print a human-readable message and exit:

```no_run
use forgedb_storage_native::DirLock;
use std::path::Path;

match DirLock::acquire(Path::new("./data")) {
    Ok(lock) => { /* proceed */ }
    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
        eprintln!("error: another ForgeDB writer already has this data directory open");
        std::process::exit(1);
    }
    Err(e) => return Err(e),
}
# Ok::<(), std::io::Error>(())
```
