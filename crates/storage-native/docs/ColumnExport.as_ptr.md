Pointer to the first byte of the exported buffer. Stable across moving
the `ColumnExport` (both a `Vec`'s heap allocation and an `Mmap`'s mapped
address are independent of where the owner struct itself lives), so the
FFI layer can capture it, then move `self` into an owner box.
