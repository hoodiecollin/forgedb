Shorten the file to `offset` bytes, dropping any tail past it, then fsync.

A no-op, with no sync, when `offset` is at or past the current length. Because
the file is in append mode the next write lands at the new end without a
cursor reset. Resets the since-fsync counters.
