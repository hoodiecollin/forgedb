Truncate the file to exactly `rows` bytes, discarding the flags of every later row.

Returns `InvalidInput` if `rows > len()`. Other errors come from `set_len`.
