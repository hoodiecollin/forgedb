Append a change to the log, assign it the next global offset, and return that
offset.

The frame is written (and synced under [`FsyncPolicy::Always`]) before the
event is broadcast to live subscribers, so a subscriber never sees an offset
the log does not hold. `model` and `bytes` are opaque and not interpreted.
Having no live subscriber is not an error; an I/O failure leaves the offset
unassigned.
