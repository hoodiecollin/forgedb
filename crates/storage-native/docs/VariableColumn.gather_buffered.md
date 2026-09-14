Bulk-load the rows at `indices` into an in-memory
[`BufferedVariableColumn`] whose `read_string(slot)` reads from memory
(#168 column scan).

Reads only the **spanned** slice of the offsets index and maps only the
**spanned** data region, so a scan over `n` variable rows pays one bounded
read plus one mapping rather than `3n` per-row `read_exact_at` syscalls
(offset + length + data). `indices` are opaque physical row positions;
reads no schema.

# Performance (#222)

This used to read the **entire** committed offsets index and the **entire**
data region into owned buffers on every call, ignoring `indices` — dead
versions included. That made a scan cost `~A x live_bytes`, a slope in
amplification rather than the fixed columns' step, and it was measured as
exactly that: at 2 000 live rows the narrow scan went 563 µs -> 5.8 ms
across A = 1 -> 16, doubling with each doubling of A.

Both reads are now bounded to `[min(indices), max(indices)]`. Append order
makes data offsets monotonic in row index, so that row span maps to one
contiguous byte span. The data span is **mapped** rather than read when it
is large enough to be worth it, which is what removes the slope: dead
versions inside the span cost address space, not I/O, because only pages
actually addressed by `read_string` are ever faulted in.

The residual cost is page granularity — a live row drags in its whole page,
including any dead bytes sharing it — so the win depends on how clustered
the live set is. Append-only helps here: an updated row is re-appended at
the tail, so churn tends to concentrate live rows rather than scatter them.

# Sparse selections (#228)

Bounding to the span is the wrong trade when the selection is a handful of
rows scattered across the column — the offsets read is then almost entirely
rows nobody asked for. Below [`SPARSE_OFFSETS_SPAN_FACTOR`] density the
offsets are read per index instead. This mirrors what
[`FixedColumn::gather`] already does via `GATHER_MMAP_MIN_ROWS`; the data
region still maps by span, which costs address space rather than I/O.

# Errors

`InvalidInput` if any index is `>= self.len()`; other errors come from the
underlying reads.
