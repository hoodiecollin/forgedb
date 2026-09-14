Per-field allocation high-water marks: an opaque map from a name to the highest value handed out. Missing from the file means empty.

This crate neither parses the keys nor branches on them; every read and write belongs to generated code, which takes `max(persisted, rescanned)` on reopen. The persisted value is a floor, not the source of truth: it exists because compaction drops dead rows, so a rescan alone could derive a lower maximum than was issued and hand a value out twice, while a crash that loses the tip safely falls back to the scan. That is what buys durability without an fsync per allocation.

Stored as a `BTreeMap` so the serialized JSON is byte-stable across writes.
