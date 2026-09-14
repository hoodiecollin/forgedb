Truncate the tombstone bitmap to hold exactly `rows` rows, discarding
everything after that point.

# Errors

Returns `Err(InvalidInput)` if `rows > self.len()`.  All other errors
come from the underlying `set_len` syscall.
