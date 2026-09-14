An in-memory commit sequencer implementing snapshot isolation with first-committer-wins.

The crate is schema-agnostic by construction: its public API names only opaque byte keys
([`OpaqueKey`]), integer sequence numbers ([`Lsn`]) and a per-transaction [`WriteSet`]. It has
no model, field or predicate types. The generated database brings the schema knowledge and
links this crate only as the ordering oracle for its transaction path.

## How it works

A transaction registers a read snapshot ([`CommitSequencer::register_snapshot`]), which
returns the LSN of the last commit at that moment. When it is ready to commit it hands the
sequencer the set of keys it wrote together with that snapshot LSN
([`CommitSequencer::try_commit`]). If any key was last committed at an LSN strictly greater
than the snapshot, another transaction won the race: the outcome is
[`CommitOutcome::Conflict`] and the caller discards its staged writes and retries. Otherwise
the sequencer assigns the next LSN, records it against every key in the write-set, and returns
[`CommitOutcome::Committed`].

Every registered snapshot is released with [`CommitSequencer::release_snapshot`], and
[`CommitSequencer::gc`] prunes conflict entries no live snapshot can still conflict with.

## Isolation level

Snapshot isolation. The disclosed anomaly is write skew: two transactions that read
overlapping keys and write disjoint subsets can both commit. The sequencer tracks write-sets
only, not read-sets.
