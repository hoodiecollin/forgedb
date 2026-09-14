Append `entry` without fsyncing, whatever the fsync policy.

Durability is deferred to a later [`Self::flush`], so a batch of appends pays
one fsync instead of one per record. A crash before that flush can lose these
records; use it only for records whose visibility already depends on a later
durable marker. See [`WalWriter::write_buffered`].
