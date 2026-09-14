Per-field allocation high-water marks, as an **opaque** `name -> highest
value handed out` map (#187).

The substrate neither parses these keys nor branches on them: it stores
and returns strings and integers. Which fields appear, what the numbers
mean, and every read and write of them belong to generated code — the
same class of layout metadata as `compaction_epoch`.

It exists because a rescan alone cannot survive compaction: compaction
physically drops dead rows, so a post-compaction reopen derives a *lower*
maximum than was actually issued and hands the same value out twice. The
contract is a **floor, not a source of truth** — a reader takes
`max(persisted, scanned)`, so a crash that loses the tip falls back to the
scan, which is always safe. That is what buys durability without an fsync
per allocation.

`BTreeMap` (not `HashMap`) so the serialized JSON is byte-stable across
writes. Additive (`#[serde(default)]`) for on-disk back-compat.
