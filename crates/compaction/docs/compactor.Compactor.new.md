Creates a compactor over `data_dir` with the given configuration.

Performs no I/O and does not check that the directory exists; a missing directory surfaces
as an error from the first compaction call.
