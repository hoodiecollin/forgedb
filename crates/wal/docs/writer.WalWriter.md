Append-only writer over a WAL file with a configurable fsync policy.

The file is opened in append mode, so every write lands at the current end.
[`Self::write`] applies the [`FsyncPolicy`]; [`Self::write_buffered`] bypasses
it; [`Self::flush`] fsyncs on demand; [`Self::truncate`] and
[`Self::truncate_to`] shorten the file. It tracks bytes and time since the last
fsync for callers that batch.
