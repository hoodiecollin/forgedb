The result of [`DurableBroker::catch_up_from`]: durable replay + a live
receiver, stitched gap-free.

Apply [`replayed`](CatchUp::replayed) in order, then drain
[`receiver`](CatchUp::receiver) ignoring any event whose `offset <=`
[`boundary`](CatchUp::boundary).
