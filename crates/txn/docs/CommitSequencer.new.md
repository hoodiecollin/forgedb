Create a new sequencer.

`start_lsn` lets the caller seed from the durable broker watermark so the
commit-LSN and broker offset form one unified monotonic sequence.  Pass `0`
for a fresh database or when seeding is not needed.

# LSN layout

Commit LSNs start at `start_lsn + 1` (the sentinel `start_lsn` represents
"before any commit").  A read snapshot taken before the first commit sees
`Lsn(start_lsn)`.  Any key committed at `Lsn(start_lsn + 1)` or later will
conflict against that snapshot, which is exactly first-committer-wins.
