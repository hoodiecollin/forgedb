Move the file cursor to absolute byte offset `pos` and return the new position.
Only [`Self::read_one`] reads from the cursor; the bulk readers rewind to the
start first.
