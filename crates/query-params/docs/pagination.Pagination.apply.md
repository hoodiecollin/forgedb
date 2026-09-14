Borrows the page `items[offset..end]`, with both bounds clamped to `items.len()`.

An `offset` past the end yields an empty slice rather than a panic. This is the step the generated REST `list` handler uses to cut the page out of the filtered and sorted rows.
