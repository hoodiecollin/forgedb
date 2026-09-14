Append an entry WITHOUT fsyncing (#170 group commit) — durability deferred
to a later [`flush`](Self::flush). Only for writes gated on a later durable
marker (MVCC-Tier-1 staged rows). See [`WalWriter::write_buffered`].
