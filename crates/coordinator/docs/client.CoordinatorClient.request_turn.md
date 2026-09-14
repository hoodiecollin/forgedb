Request a commit turn for the given write-set keys + snapshot LSN.

Returns `Ok(Grant { turn_id, reserved_lsn })` on success, or one of the
error variants on conflict/busy/IO failure.
