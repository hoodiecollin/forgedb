Acquire the data-directory lock, open/create the replication log, seed
the `CommitSequencer` from the watermark, and return a `Coordinator`
ready to call [`run`](Coordinator::run).

Returns `Err(DirAlreadyLocked)` if another coordinator process already
holds the lock on this directory.
