Append the framed entry without fsyncing, whatever the policy.

Durability is deferred to an explicit [`Self::flush`] at the batch or
transaction boundary, so N appends pay one barrier instead of N. A crash before
that flush loses these records, which is only correct for records whose
visibility is gated on a later durable marker, such as staged transaction rows
that recovery discards unless the transaction committed. The bytes still count
toward [`Self::bytes_since_fsync`].
