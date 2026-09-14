Open a coordinator for `root` with [`CoordConfig::default`].

Creates `root` if needed, takes the exclusive lock on `<root>/`[`DIR_LOCK_FILENAME`], opens or creates `<root>/_coordinator_replication.log`, and seeds the commit sequencer from that log's watermark. The socket is not bound until [`Self::run`].

Returns [`ServerError::DirAlreadyLocked`] if another coordinator or a standalone writer holds the lock, and [`ServerError::Io`] for any other filesystem failure.
