Spawns the scheduling thread. No-op if already started.

The thread sleeps `check_interval_secs`, then, if not stopped, sets the status to
[`CompactionStatus::Running`], calls [`Compactor::compact_needed`], stores the results for
[`Self::last_results`], and sets [`CompactionStatus::Completed`] or
[`CompactionStatus::Failed`]; it repeats until [`Self::stop`]. The first pass runs one full
interval after `start`. `auto_compact` is not consulted. When the thread exits it sets the
status back to [`CompactionStatus::Idle`].

Calling `start` again after [`Self::stop`] while the previous thread is still sleeping
keeps that thread alive and spawns a second one; drop the value to join before restarting.
