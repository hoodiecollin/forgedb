Read row `index` as an owned `String`.

Reads the row's `(offset, length)` entry from the offsets file, then its bytes from the data file. Returns `InvalidInput` if `index >= len()` and `InvalidData` if the bytes are not valid UTF-8.
