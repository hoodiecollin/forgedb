Runs [`Compactor::compact_needed`] on a background thread at a fixed interval.

Not linked by generated code, which compacts in-process under its own writer lock. Every
pass routes through the deprecated tombstone-based [`Compactor::compact_model`]. The thread
takes no lock on the data directory, so it must not run beside a writer.

[`Self::start`] spawns the thread, [`Self::stop`] asks it to exit, and dropping the value
joins it. [`Self::status`] and [`Self::last_results`] expose the outcome of the most recent
pass; [`Self::trigger_manual`] runs one pass immediately on its own thread.
