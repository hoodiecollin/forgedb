Fsync on an append when at least this much time has elapsed since the last
fsync. The check runs only when an append happens: a record written before the
interval elapses is not synced by that write, and nothing syncs while the log is
idle.
