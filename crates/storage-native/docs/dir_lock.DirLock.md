An exclusive advisory lock on a ForgeDB data directory, held for as long as the value lives.

Holds an open `File` on `<root>/.forgedb.lock` with an OS-level exclusive advisory lock taken by `fs2::FileExt::try_lock_exclusive`. Dropping the value closes the file, which releases the lock; the lock file itself is never deleted.

It prevents two writers from opening one directory by accident. It is not a lease, a registry or a distributed coordinator, and it does not serialize concurrent writers. The `forgedb-coordinator` process locks the same file, so a coordinator and a standalone writer exclude each other.

Acquire it with [`DirLock::acquire`]; a directory already locked through any other handle, in this process or another, yields `WouldBlock`.
