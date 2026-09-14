Atomically write this manifest to `path` via temp-file + `fsync` +
`rename`. A crash mid-write leaves the intact previous manifest rather
than a truncated/garbage one that would fail to parse on the next open.
The temp file is `<path>.tmp`.
