Compacts every model under the data directory through the deprecated
[`Self::compact_model`].

Every subdirectory whose name does not start with `.` is treated as a model. A model whose
compaction fails contributes a [`CompactionResult`] with `success == false` and the message
in `error` rather than aborting the run. Returns `Err` only when the data directory itself
cannot be listed.
