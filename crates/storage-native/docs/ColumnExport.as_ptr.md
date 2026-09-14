Pointer to the first byte of the buffer.

Stable across moving the `ColumnExport` itself: a `Vec`'s heap allocation and an `Mmap`'s mapped address are independent of where the owner lives, so FFI glue can capture the pointer and then move `self` into an owner box.
