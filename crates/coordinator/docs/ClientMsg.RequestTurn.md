Request a commit turn.

The coordinator conflict-checks `write_set_keys` against its in-memory
conflict map (`CommitSequencer`), then either grants an exclusive turn or
nacks.  No shared column state is touched here — that is the data plane,
solely the client's responsibility.
