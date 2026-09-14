MVCC Tier 2 commit sequencer for ForgeDB (#83).

This crate is **structurally schema-agnostic**: its public API names only
rows, opaque byte-keys, and integer LSNs.  No model name, no field, no
generic predicate, no `Fn`-over-schema.  It is the substrate the generated
`Database::transaction_retrying` links against — the generated code brings
the schema knowledge; this crate provides only the ordering oracle.

## Design

The [`CommitSequencer`] implements **snapshot isolation with first-committer-wins**
(SI-FCW): the first transaction to commit a key at a given LSN wins; a later
transaction whose read snapshot predates that commit conflicts and must retry.

Isolation level: **snapshot isolation**.  The disclosed anomaly is write-skew
(two transactions read overlapping key sets, each writes a disjoint subset;
both may commit in SI).  Serializable snapshot isolation (SSI) requires
read-set tracking and is deferred to Tier 3.

## Usage

```
use forgedb_txn::{CommitSequencer, WriteSet, CommitOutcome, Lsn};

// new(0) → commit LSNs start at Lsn(1); sentinel "before any commit" = Lsn(0).
let mut seq = CommitSequencer::new(0);

// Transaction A takes a snapshot before any commit → snap_a = Lsn(0).
let snap_a = seq.register_snapshot();
assert_eq!(snap_a, Lsn(0));

// Transaction B takes a snapshot at the same point → snap_b = Lsn(0).
let snap_b = seq.register_snapshot();

// A commits key "row:0" → assigned Lsn(1).
let ws_a = WriteSet {
    keys: vec![b"row:0".to_vec().into_boxed_slice()],
    snapshot_lsn: snap_a,
};
match seq.try_commit(&ws_a) {
    CommitOutcome::Committed(lsn) => {
        println!("A committed at LSN {}", lsn.as_u64());
        assert_eq!(lsn, Lsn(1));
    }
    CommitOutcome::Conflict { .. } => panic!("unexpected conflict"),
}
seq.release_snapshot(snap_a);

// B tries to commit the same key with snap_b = Lsn(0).
// A committed at Lsn(1) > Lsn(0) → conflict.
let ws_b = WriteSet {
    keys: vec![b"row:0".to_vec().into_boxed_slice()],
    snapshot_lsn: snap_b,
};
match seq.try_commit(&ws_b) {
    CommitOutcome::Conflict { .. } => println!("B conflicts, must retry"),
    CommitOutcome::Committed(_) => panic!("should have conflicted"),
}
seq.release_snapshot(snap_b);
```
