The slot's value as an owned `String`. Kept for callers that genuinely need
ownership; it is [`read_str`](Self::read_str) plus the copy, so there is one
decode path and the two can never disagree.
