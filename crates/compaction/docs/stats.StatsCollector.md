Reads row counts and byte usage from a data directory without modifying it.

Holds only the data directory. Every method derives a model's row count from the size of
its `tombstones.bin` and treats a nonzero byte as a deleted row. See [`crate`] for the
layout.
