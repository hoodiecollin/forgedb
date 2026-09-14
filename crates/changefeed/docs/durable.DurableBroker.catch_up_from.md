Race-free resume: subscribe to the live tail, then replay everything
durably retained after `after` up to the current watermark.

Returns a [`CatchUp`] carrying the replayed events, the `boundary` offset
they were replayed through, and a live `receiver`. The caller applies
`replayed` first, then drains `receiver` **skipping any event whose
`offset <= boundary`** — those were already covered by the replay, and
skipping them is exactly the "idempotent by absolute offset" rule. No
event in `(after, ∞)` is dropped: the receiver was subscribed before the
boundary was read, so any offset `> boundary` is guaranteed to arrive
live.

Callers hold the single-writer discipline (this is `&self`; `record` is
`&mut self`), so no `record` interleaves the subscribe/replay pair.
