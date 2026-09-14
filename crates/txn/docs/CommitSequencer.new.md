Creates a sequencer whose first commit will be assigned `Lsn(start_lsn + 1)`.

`Lsn(start_lsn)` is the "before any commit" sentinel: a snapshot registered before the first
commit observes it, and any key committed afterwards conflicts with that snapshot, which is
exactly first-committer-wins. Pass `0` for a fresh database, or a durable offset when the
commit LSN and another monotonic sequence must form one line.
