Whether an append is followed by an fsync.

The policy is consulted only by [`WalManager::write`]; there is no background
timer. [`WalManager::write_buffered`] bypasses it and [`WalManager::flush`] syncs
regardless of it. An fsync here is `File::sync_all`, so file metadata is synced
along with the data.
