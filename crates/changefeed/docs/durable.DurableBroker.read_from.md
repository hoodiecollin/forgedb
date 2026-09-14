Read the retained events with `offset > after`, in offset order, at most
`max` of them.

A cold follower passes `after = 0`. Returns fewer than `max` (possibly none)
when the end of the log is reached, and nothing when `max` is `0`. Each call
reads the whole log file from the front, so the cost is proportional to
everything retained, not to the events returned. A frame with a bad checksum
is an `InvalidData` error; an incomplete trailing frame ends the scan
silently.
