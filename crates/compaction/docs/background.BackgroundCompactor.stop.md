Asks the scheduling thread to exit. Returns immediately.

The thread notices the request when its current sleep ends, so it may keep running for up
to `check_interval_secs` plus any pass in progress. Dropping the value joins it.
[`Self::is_running`] reports `false` as soon as `stop` returns.
