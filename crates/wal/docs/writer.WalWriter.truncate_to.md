Truncate the WAL to `offset` bytes, dropping any tail past it (MVCC Tier 1
transaction rollback).  A no-op if `offset` is already at or past the
current length.  The file is opened in append mode, so no cursor reset is
needed — the next write still appends at the (new, shorter) end.
