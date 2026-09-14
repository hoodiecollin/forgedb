A monotonic logical sequence number.

LSNs are assigned strictly in commit order.  `Lsn(0)` is the "before any
commit" sentinel.  The sequencer starts at `start_lsn`; on `open_at` that
is seeded from the durable broker watermark so the commit-LSN and the
broker's global offset form one unified monotonic sequence.
