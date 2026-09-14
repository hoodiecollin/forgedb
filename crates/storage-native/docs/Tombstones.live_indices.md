Return the subset of `rows` that are not flagged deleted, preserving order.

Reads the whole file once rather than issuing one positional read per row, which is the bulk liveness filter a column scan applies to its selection. A row index at or beyond the count is dropped, matching `is_deleted(..).unwrap_or(true)`.
