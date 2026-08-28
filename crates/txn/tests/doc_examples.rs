use forgedb_txn::{CommitOutcome, CommitSequencer, Lsn, WriteSet};

#[test]
fn second_committer_of_the_same_key_conflicts() {
    let mut seq = CommitSequencer::new(0);

    let snap_a = seq.register_snapshot();
    assert_eq!(snap_a, Lsn(0));

    let snap_b = seq.register_snapshot();

    let ws_a = WriteSet {
        keys: vec![b"row:0".to_vec().into_boxed_slice()],
        snapshot_lsn: snap_a,
    };
    match seq.try_commit(&ws_a) {
        CommitOutcome::Committed(lsn) => assert_eq!(lsn, Lsn(1)),
        CommitOutcome::Conflict { .. } => panic!("unexpected conflict"),
    }
    seq.release_snapshot(snap_a);

    let ws_b = WriteSet {
        keys: vec![b"row:0".to_vec().into_boxed_slice()],
        snapshot_lsn: snap_b,
    };
    match seq.try_commit(&ws_b) {
        CommitOutcome::Conflict { .. } => {}
        CommitOutcome::Committed(_) => panic!("should have conflicted"),
    }
    seq.release_snapshot(snap_b);
}
