A write–write conflict: `key` was last committed by a transaction whose
commit LSN is strictly greater than the attempting transaction's
`snapshot_lsn`.  The caller should discard the staged writes and retry.
