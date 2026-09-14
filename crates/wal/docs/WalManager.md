One WAL file: append, fsync, replay, truncate and rotate.

Wraps a [`WalWriter`] and a [`WalReader`] on the same path. [`Self::open`]
creates the file if needed; [`Self::write`] and [`Self::write_buffered`] append;
[`Self::replay`] is the crash-recovery entry point; [`Self::truncate`],
[`Self::truncate_to`] and [`Self::rotate`] shorten or archive the file. The
manager takes no lock on the file; single-writer discipline is the caller's.
