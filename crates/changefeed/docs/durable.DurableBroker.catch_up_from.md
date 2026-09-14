Resume without a gap: subscribe to the live tail, then replay everything
retained after `after`.

Returns a [`CatchUp`] holding the replayed events, the `boundary` they were
replayed through (the watermark at the time of the call), and the live
`receiver`, which was subscribed before that watermark was read. The caller
applies `replayed` in order, then drains `receiver` skipping every event with
`offset <= boundary`, which the replay already covered. Any offset above
`boundary` is guaranteed to arrive on the receiver.

`max` caps the replay exactly as in [`DurableBroker::read_from`]. If it stops
the replay short of `boundary`, the events between the last replayed offset
and `boundary` reach the caller on neither path, so pass a `max` that reaches
the watermark (the generated `/replicate` handler passes `usize::MAX`).

This takes `&self` while [`DurableBroker::record`] takes `&mut self`, so no
record can interleave between the subscribe and the replay.
