The value at `slot`, borrowed from the buffer with no allocation or copy.

The borrow is tied to `&self`, and the buffer (an owned `Vec` or an `mmap`) is owned by `self`, so the `&str` cannot outlive the bytes it points at. UTF-8 is validated on every read, so on-disk corruption surfaces as an error rather than as an unchecked `&str`.

Returns `InvalidInput` if `slot >= len()`, and `InvalidData` if the slot's range falls outside the buffer or its bytes are not valid UTF-8.
