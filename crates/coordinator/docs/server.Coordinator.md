A running coordinator for one data directory: the directory lock, the socket path, and the shared turn and broker state.

Construct it with [`Self::open`], [`Self::open_with_fsync`] or [`Self::open_with_config`], then call [`Self::run`]. Dropping it closes the lock file, releasing the directory lock. `Debug` prints only the root and the socket path.
