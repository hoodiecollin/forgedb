Bulk-load the physical rows at `indices` into an in-memory
[`BufferedFixedColumn`] whose positional `read_*(slot)` accessors read
from memory instead of the file (#168 column scan).

This is the class-1 read primitive behind the generated column-pruned
sequential scan: a scan over `n` rows pays **one** bulk read per column
(a zero-copy `mmap` alias when `indices` is the dense prefix `[0, n)` — the
common append-only case — or a single gathered copy otherwise) rather than
`n` per-row `read_exact_at` syscalls.  `indices` are opaque physical row
positions the caller (generated code) computed from the live set; the
returned buffer's `read_*` methods take a **slot** `0..indices.len()`
indexing into that selection, in the given order.  Reads no field name,
type, or schema.

# Errors

Propagates [`export`](Self::export)'s errors (out-of-bounds index or a
failed `mmap`).
