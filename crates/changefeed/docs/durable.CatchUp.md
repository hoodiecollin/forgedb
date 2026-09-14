The result of [`DurableBroker::catch_up_from`]: a durable replay and a live
receiver, stitched without a gap.

Apply [`replayed`](CatchUp::replayed) in order, then drain
[`receiver`](CatchUp::receiver), ignoring any event whose `offset` is at or
below [`boundary`](CatchUp::boundary).
