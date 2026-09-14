Read-only, positional view over a [`FixedColumn`]'s backing file
(#56 Direction B — single writer, many concurrent readers).

Created by [`FixedColumn::reader`]; holds an independent file descriptor
(`try_clone`) to the **same** file as the writer.  A single `&mut self`
writer can keep appending while any number of `&self` readers positionally
read the committed prefix concurrently, with no lock:

- Reads use `read_exact_at` (positional `pread`) — they never touch a shared
  file cursor, so they are safe against a concurrent seek-based append and
  against each other across threads.
- The engine is append-only: bytes at an already-committed offset never move,
  so a reader observing an offset below the writer's committed length reads
  stable bytes (POSIX page-cache coherence makes the writer's not-yet-fsync'd
  appends visible through this independent fd).
- Length is derived **live** on every access (never a cached count), so a
  reader created before an append still sees rows the writer commits
  afterward — once the caller's watermark admits them.

Callers clamp reads to a captured watermark (the row-count anchor); because
the anchor column is appended last per row, a row within the watermark has
all its columns fully written, so a reader never observes a torn row.
