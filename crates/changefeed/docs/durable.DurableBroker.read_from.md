Read persisted events with `offset > after`, up to `max` of them, in
offset order.

A cold follower passes `after = 0` to read from the beginning of the
retained log. Returns fewer than `max` (possibly zero) when the tail is
reached. Scans the log from the front — O(retained) per call, acceptable
for the infrequent catch-up path at v1's application-dataset scale.
