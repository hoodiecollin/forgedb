Roll the file back to `offset` bytes, discarding everything appended after
that point.

Pair it with a [`Self::size`] taken before a batch of appends: the earlier
records survive and only the batch is dropped, which is how a transaction's
staged records are rolled back without clearing the log. The shortened file is
fsynced and the reader is reopened. A no-op when `offset` is at or past the
current length. An `offset` that does not fall on a record boundary leaves a
torn record at the tail, which [`Self::replay`] then treats as the end of the
log.
