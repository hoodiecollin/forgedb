Write this manifest to `path` atomically.

Serializes to pretty-printed JSON, creates any missing parent directories, writes `<path>.tmp`, calls `sync_all` on it, then renames it over `path`. A crash mid-write leaves the previous manifest intact rather than a truncated one that would fail to parse on the next open.
