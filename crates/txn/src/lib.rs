use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lsn(pub u64);

impl Lsn {
    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

pub type OpaqueKey = Box<[u8]>;

pub struct WriteSet {
    pub keys: Vec<OpaqueKey>,
    pub snapshot_lsn: Lsn,
}

impl WriteSet {
    pub fn new(snapshot_lsn: Lsn) -> Self {
        WriteSet {
            keys: Vec::new(),
            snapshot_lsn,
        }
    }
}

pub enum CommitOutcome {
    Committed(Lsn),
    Conflict {
        key: OpaqueKey,
    },
}

pub struct CommitSequencer {
    next_lsn: u64,
    conflicts: HashMap<OpaqueKey, Lsn>,
    live_snapshots: BTreeMap<Lsn, u32>,
}

impl CommitSequencer {
    pub fn new(start_lsn: u64) -> Self {
        CommitSequencer {
            next_lsn: start_lsn + 1,
            conflicts: HashMap::new(),
            live_snapshots: BTreeMap::new(),
        }
    }

    pub fn register_snapshot(&mut self) -> Lsn {
        let snap = Lsn(self.next_lsn - 1);
        *self.live_snapshots.entry(snap).or_insert(0) += 1;
        snap
    }

    pub fn release_snapshot(&mut self, s: Lsn) {
        if let Some(count) = self.live_snapshots.get_mut(&s) {
            if *count <= 1 {
                self.live_snapshots.remove(&s);
            } else {
                *count -= 1;
            }
        }
    }

    pub fn oldest_live_snapshot(&self) -> Lsn {
        self.live_snapshots
            .keys()
            .next()
            .copied()
            .unwrap_or(Lsn(0))
    }

    pub fn try_commit(&mut self, ws: &WriteSet) -> CommitOutcome {
        for k in &ws.keys {
            if matches!(self.conflicts.get(k), Some(&l) if l > ws.snapshot_lsn) {
                return CommitOutcome::Conflict { key: k.clone() };
            }
        }
        let l = Lsn(self.next_lsn);
        self.next_lsn += 1;
        for k in &ws.keys {
            self.conflicts.insert(k.clone(), l);
        }
        CommitOutcome::Committed(l)
    }

    pub fn gc(&mut self) {
        let oldest = self.oldest_live_snapshot();
        if oldest == Lsn(0) && self.live_snapshots.is_empty() {
            self.conflicts.clear();
            return;
        }
        self.conflicts.retain(|_k, &mut commit_lsn| commit_lsn >= oldest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: &[u8]) -> OpaqueKey {
        b.to_vec().into_boxed_slice()
    }

    #[test]
    fn first_committer_wins_conflict() {
        let mut seq = CommitSequencer::new(0);

        let snap_a = seq.register_snapshot();
        let snap_b = seq.register_snapshot();

        let ws_a = WriteSet {
            keys: vec![key(b"row:0")],
            snapshot_lsn: snap_a,
        };
        let CommitOutcome::Committed(lsn_a) = seq.try_commit(&ws_a) else {
            panic!("A should commit");
        };
        assert_eq!(lsn_a, Lsn(1));

        let ws_b = WriteSet {
            keys: vec![key(b"row:0")],
            snapshot_lsn: snap_b,
        };
        let CommitOutcome::Conflict { key: ck } = seq.try_commit(&ws_b) else {
            panic!("B should conflict");
        };
        assert_eq!(&*ck, b"row:0");

        seq.release_snapshot(snap_a);
        seq.release_snapshot(snap_b);
    }

    #[test]
    fn no_conflict_when_key_committed_before_snapshot() {
        let mut seq = CommitSequencer::new(0);

        let snap_a = seq.register_snapshot();

        let ws_a = WriteSet {
            keys: vec![key(b"row:0")],
            snapshot_lsn: snap_a,
        };
        let CommitOutcome::Committed(c) = seq.try_commit(&ws_a) else {
            panic!();
        };
        assert_eq!(c, Lsn(1));
        seq.release_snapshot(snap_a);

        let snap_b = seq.register_snapshot();
        assert_eq!(snap_b, Lsn(1));

        let ws_b = WriteSet {
            keys: vec![key(b"row:0")],
            snapshot_lsn: snap_b,
        };
        assert!(
            matches!(seq.try_commit(&ws_b), CommitOutcome::Committed(_)),
            "B should commit — A's commit is not strictly after B's snapshot"
        );
        seq.release_snapshot(snap_b);
    }

    #[test]
    fn snapshot_refcount_basic() {
        let mut seq = CommitSequencer::new(0);

        assert_eq!(seq.oldest_live_snapshot(), Lsn(0));

        let s1 = seq.register_snapshot();
        assert_eq!(s1, Lsn(0));
        assert_eq!(seq.oldest_live_snapshot(), Lsn(0));

        let ws = WriteSet {
            keys: vec![key(b"k")],
            snapshot_lsn: s1,
        };
        let CommitOutcome::Committed(c) = seq.try_commit(&ws) else {
            panic!();
        };
        assert_eq!(c, Lsn(1));

        let s2 = seq.register_snapshot();
        assert_eq!(s2, Lsn(1));

        seq.release_snapshot(s1);
        assert_eq!(seq.oldest_live_snapshot(), Lsn(1));

        seq.release_snapshot(s2);
        assert!(seq.live_snapshots.is_empty());
    }

    #[test]
    fn multiple_live_snapshots_oldest_correct() {
        let mut seq = CommitSequencer::new(0);

        let snap0 = seq.register_snapshot();
        assert_eq!(snap0, Lsn(0));

        let ws1 = WriteSet {
            keys: vec![key(b"a")],
            snapshot_lsn: snap0,
        };
        let CommitOutcome::Committed(c1) = seq.try_commit(&ws1) else {
            panic!()
        };
        assert_eq!(c1, Lsn(1));

        let snap1 = seq.register_snapshot();
        assert_eq!(snap1, Lsn(1));

        let ws2 = WriteSet {
            keys: vec![key(b"b")],
            snapshot_lsn: snap1,
        };
        let CommitOutcome::Committed(c2) = seq.try_commit(&ws2) else {
            panic!()
        };
        assert_eq!(c2, Lsn(2));

        let snap2 = seq.register_snapshot();
        assert_eq!(snap2, Lsn(2));

        assert_eq!(seq.oldest_live_snapshot(), Lsn(0));

        seq.release_snapshot(snap0);
        assert_eq!(seq.oldest_live_snapshot(), Lsn(1));

        seq.release_snapshot(snap1);
        assert_eq!(seq.oldest_live_snapshot(), Lsn(2));

        seq.release_snapshot(snap2);
        assert!(seq.live_snapshots.is_empty());
    }

    #[test]
    fn gc_prunes_old_entries() {
        let mut seq = CommitSequencer::new(0);

        let s0 = seq.register_snapshot();
        for i in 0u8..5 {
            let ws = WriteSet {
                keys: vec![vec![i].into_boxed_slice()],
                snapshot_lsn: s0,
            };
            assert!(matches!(seq.try_commit(&ws), CommitOutcome::Committed(_)));
        }
        assert_eq!(seq.conflicts.len(), 5);

        seq.release_snapshot(s0);

        seq.gc();
        assert!(seq.conflicts.is_empty(), "gc should prune all entries with no live snapshots");
    }

    #[test]
    fn gc_keeps_entries_needed_by_snapshot() {
        let mut seq = CommitSequencer::new(0);

        let s_before = seq.register_snapshot();

        let ws_a = WriteSet {
            keys: vec![key(b"A")],
            snapshot_lsn: s_before,
        };
        let CommitOutcome::Committed(ca) = seq.try_commit(&ws_a) else {
            panic!()
        };
        assert_eq!(ca, Lsn(1));

        let s_after_a = seq.register_snapshot();

        let ws_b = WriteSet {
            keys: vec![key(b"B")],
            snapshot_lsn: s_after_a,
        };
        let CommitOutcome::Committed(cb) = seq.try_commit(&ws_b) else {
            panic!()
        };
        assert_eq!(cb, Lsn(2));

        seq.release_snapshot(s_before);
        seq.gc();
        assert_eq!(seq.conflicts.len(), 2, "both entries still needed");

        seq.release_snapshot(s_after_a);
        seq.gc();
        assert!(seq.conflicts.is_empty());
    }
}
