Truncate the column to exactly `rows` rows, discarding everything after them.

The offsets file is cut to `rows * 16` bytes and the data file to the byte after the end of row `rows - 1` (read from that row's offsets entry), or to zero when `rows == 0`. Both in-memory counters are updated so the next append lands at exactly row `rows`.

Returns `InvalidInput` if `rows > len()`. Other errors come from reading the offsets entry or from `set_len`.
