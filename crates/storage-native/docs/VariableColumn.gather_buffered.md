Load the rows at `indices` into a [`BufferedVariableColumn`] whose reads slice from memory.

`indices` are opaque physical row positions; the column reads no schema. Slot `i` of the result is row `indices[i]`, and an empty `indices` returns an empty buffer.

For a dense selection the offsets entries from `min(indices)` to `max(indices)` are read with one positional read, and the data bytes between the lowest and highest referenced offset are loaded as one span: mapped with `mmap` when the span is at least 64 KiB (falling back to a read if the mapping fails), read into an owned buffer otherwise. Dead rows inside a mapped span cost address space rather than I/O, because only the pages a read touches are faulted in.

A selection is treated as sparse when its row span exceeds `128 * indices.len()`. Then each row's offsets entry and bytes are read individually and packed into one owned buffer, so the offsets read does not scan rows nobody asked for; nothing is mapped on this path.

Returns `InvalidInput` if any index is `>= len()`; other errors come from the underlying reads.
