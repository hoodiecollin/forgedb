Append `entry` and apply the fsync policy.

Delegates to [`WalWriter::write`]: the framed record is appended, then fsynced
according to the [`FsyncPolicy`] given to [`Self::open`].
