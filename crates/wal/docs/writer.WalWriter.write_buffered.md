Append an entry WITHOUT fsyncing, regardless of the fsync policy (#170
group commit).  Durability is deferred to an explicit [`flush`](Self::flush)
at the batch/transaction commit boundary — so a batch of N appends pays one
barrier instead of N.  Safe only for writes whose visibility is gated on a
later durable marker (e.g. MVCC-Tier-1 staged rows, which crash-recovery
drops unless the transaction journal committed): a crash before the flush
loses these appends, which is exactly correct for not-yet-committed data.
