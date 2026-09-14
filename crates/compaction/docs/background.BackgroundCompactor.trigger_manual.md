Runs one [`Compactor::compact_needed`] pass on a fresh thread and returns without waiting
for it.

Returns `Err` if the status is already [`CompactionStatus::Running`]; the check and the
transition to `Running` happen under one lock, so two concurrent callers cannot both spawn.
Does not require [`Self::start`]. The outcome is visible through [`Self::status`] and
[`Self::last_results`]; the thread is detached and is not joined on drop.
