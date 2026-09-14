Load the rows at `indices` into a [`BufferedFixedColumn`] whose `read_*(slot)` accessors read from memory instead of the file.

A column scan over `n` rows then pays one bulk read per column (an `mmap` alias when `indices` is the dense prefix, a single gathered copy otherwise) rather than `n` positional reads. Slot `i` of the result is row `indices[i]`.

Propagates the errors of [`FixedColumn::export`]: `InvalidInput` for an index `>= len()`, or a failed mapping.
