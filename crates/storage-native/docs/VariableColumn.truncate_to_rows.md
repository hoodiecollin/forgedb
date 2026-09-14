Truncate the column to hold exactly `rows` rows, discarding everything
after that point.

The data file is cut at the byte immediately after the last byte of row
`rows - 1` (read from the offsets entry for that row).  The offsets file
is cut to `rows * 16` bytes.  Both in-memory counters are updated so the
next `append_string` lands at exactly row `rows`.

# Errors

Returns `Err(InvalidInput)` if `rows > self.len()`.  All other errors
come from underlying I/O or `set_len` syscalls.
