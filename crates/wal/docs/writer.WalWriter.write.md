Append the framed entry, then apply the fsync policy.

[`FsyncPolicy::Always`] syncs on every call, [`FsyncPolicy::Periodic`] syncs
when the configured interval has elapsed since the last sync, and
[`FsyncPolicy::Never`] does not sync. A sync resets
[`Self::bytes_since_fsync`] and [`Self::time_since_fsync`].
