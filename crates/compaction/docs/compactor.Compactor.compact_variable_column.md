Compact a single variable-length column.

Writes a temp file and renames atomically.  Part of the public API for
direct per-column use; `compact_model` uses the staging variant instead.
