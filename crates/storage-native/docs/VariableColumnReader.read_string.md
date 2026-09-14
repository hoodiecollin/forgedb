Read row `index` as an owned `String`.

Returns `InvalidInput` if the row's offsets entry lies past the end of the offsets file, the I/O error if its bytes are not fully present in the data file, and `InvalidData` if they are not valid UTF-8.
