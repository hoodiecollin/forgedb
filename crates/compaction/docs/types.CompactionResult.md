Outcome of compacting one model.

Byte figures are the model's [`ModelStats::total_disk_bytes`] measured before and after the
pass. A result returned directly by [`crate::Compactor::compact_model_keeping`] or
[`crate::Compactor::compact_model`] always has `success == true`; the batch methods
[`crate::Compactor::compact_all`] and [`crate::Compactor::compact_needed`] convert a
per-model failure into a result with `success == false`, zeroed byte fields and the message
in `error`.
