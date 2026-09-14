Measures every model under the data directory and sums the byte totals.

Every subdirectory whose name does not start with `.` is treated as a model. A model whose
statistics cannot be collected is logged at `warn` and left out of `models`, so a partially
readable directory still yields a result. `collected_at` is taken before the walk begins.
Returns `Err` only when the data directory itself cannot be listed.
