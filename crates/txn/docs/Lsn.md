A monotonic logical sequence number assigned in commit order.

`Lsn(0)` is the "before any commit" sentinel of a sequencer created with
`CommitSequencer::new(0)`; a sequencer seeded with a higher `start_lsn` uses that value as its
sentinel instead. Ordering and equality are on the inner integer.
