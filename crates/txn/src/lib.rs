//! MVCC Tier 2 commit sequencer for ForgeDB (#83).
//!
//! This crate is **structurally schema-agnostic**: its public API names only
//! rows, opaque byte-keys, and integer LSNs.  No model name, no field, no
//! generic predicate, no `Fn`-over-schema.  It is the substrate the generated
//! `Database::transaction_retrying` links against — the generated code brings
//! the schema knowledge; this crate provides only the ordering oracle.
//!
//! ## Design
//!
//! The [`CommitSequencer`] implements **snapshot isolation with first-committer-wins**
//! (SI-FCW): the first transaction to commit a key at a given LSN wins; a later
//! transaction whose read snapshot predates that commit conflicts and must retry.
//!
//! Isolation level: **snapshot isolation**.  The disclosed anomaly is write-skew
//! (two transactions read overlapping key sets, each writes a disjoint subset;
//! both may commit in SI).  Serializable snapshot isolation (SSI) requires
//! read-set tracking and is deferred to Tier 3.
//!
//! ## Usage
//!
//! ```
//! use forgedb_txn::{CommitSequencer, WriteSet, CommitOutcome, Lsn};
//!
//! // new(0) → commit LSNs start at Lsn(1); sentinel "before any commit" = Lsn(0).
//! let mut seq = CommitSequencer::new(0);
//!
//! // Transaction A takes a snapshot before any commit → snap_a = Lsn(0).
//! let snap_a = seq.register_snapshot();
//! assert_eq!(snap_a, Lsn(0));
//!
//! // Transaction B takes a snapshot at the same point → snap_b = Lsn(0).
//! let snap_b = seq.register_snapshot();
//!
//! // A commits key "row:0" → assigned Lsn(1).
//! let ws_a = WriteSet {
//!     keys: vec![b"row:0".to_vec().into_boxed_slice()],
//!     snapshot_lsn: snap_a,
//! };
//! match seq.try_commit(&ws_a) {
//!     CommitOutcome::Committed(lsn) => {
//!         println!("A committed at LSN {}", lsn.as_u64());
//!         assert_eq!(lsn, Lsn(1));
//!     }
//!     CommitOutcome::Conflict { .. } => panic!("unexpected conflict"),
//! }
//! seq.release_snapshot(snap_a);
//!
//! // B tries to commit the same key with snap_b = Lsn(0).
//! // A committed at Lsn(1) > Lsn(0) → conflict.
//! let ws_b = WriteSet {
//!     keys: vec![b"row:0".to_vec().into_boxed_slice()],
//!     snapshot_lsn: snap_b,
//! };
//! match seq.try_commit(&ws_b) {
//!     CommitOutcome::Conflict { .. } => println!("B conflicts, must retry"),
//!     CommitOutcome::Committed(_) => panic!("should have conflicted"),
//! }
//! seq.release_snapshot(snap_b);
//! ```

use std::collections::{BTreeMap, HashMap};

/// A monotonic logical sequence number.
///
/// LSNs are assigned strictly in commit order.  `Lsn(0)` is the "before any
/// commit" sentinel.  The sequencer starts at `start_lsn`; on `open_at` that
/// is seeded from the durable broker watermark so the commit-LSN and the
/// broker's global offset form one unified monotonic sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lsn(pub u64);

impl Lsn {
    /// Return the raw integer value.
    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// An opaque conflict key — arbitrary bytes identifying one row or unique-key claim.
///
/// The sequencer never interprets the bytes; it compares them only for equality
/// to detect write–write conflicts.
pub type OpaqueKey = Box<[u8]>;

/// The write-set a transaction hands to [`CommitSequencer::try_commit`].
///
/// `keys` is the complete set of opaque row and unique-key handles the
/// transaction touched.  `snapshot_lsn` is the LSN at which the transaction
/// took its read snapshot (i.e., `CommitSequencer::register_snapshot`'s return
/// value); any key last committed at a strictly-higher LSN is a conflict.
pub struct WriteSet {
    /// Every opaque key touched by this transaction's writes.
    pub keys: Vec<OpaqueKey>,
    /// The read snapshot LSN this transaction observed.
    pub snapshot_lsn: Lsn,
}

impl WriteSet {
    /// Convenience constructor.
    pub fn new(snapshot_lsn: Lsn) -> Self {
        WriteSet {
            keys: Vec::new(),
            snapshot_lsn,
        }
    }
}

/// The result of a [`CommitSequencer::try_commit`] attempt.
pub enum CommitOutcome {
    /// The transaction committed successfully.  The inner [`Lsn`] is its
    /// globally-unique, monotonically-increasing commit LSN.
    Committed(Lsn),
    /// A write–write conflict: `key` was last committed by a transaction whose
    /// commit LSN is strictly greater than the attempting transaction's
    /// `snapshot_lsn`.  The caller should discard the staged writes and retry.
    Conflict {
        /// The first conflicting opaque key detected.
        key: OpaqueKey,
    },
}

/// In-memory commit sequencer implementing snapshot isolation with
/// first-committer-wins (SI-FCW, #83 Tier 2).
///
/// The sequencer is intentionally **in-memory and ephemeral**: it rebuilds
/// empty on process restart.  In-flight transactions do not survive a restart
/// (they are rolled back by the Tier-1 WAL journal), so the conflict map does
/// not need to be persisted.
///
/// `CommitSequencer` is behind `Arc<Mutex<..>>` in the generated `Database`
/// so that a future Tier 3 path may hold the lock only for the serialized
/// commit point rather than across the entire prepare phase.
pub struct CommitSequencer {
    /// The LSN that will be assigned to the NEXT successful commit.
    next_lsn: u64,
    /// `opaque_key → LSN of its last committer`.
    ///
    /// Used for conflict detection: if a key's recorded LSN is strictly greater
    /// than a challenger's `snapshot_lsn`, the challenger conflicts.  Bounded
    /// by [`gc`](CommitSequencer::gc) once entries are older than the oldest live snapshot.
    conflicts: HashMap<OpaqueKey, Lsn>,
    /// Reference-counted open read snapshots.
    ///
    /// `Lsn → refcount`.  Maintained via [`register_snapshot`] /
    /// [`release_snapshot`]; used by [`oldest_live_snapshot`] for GC bounds
    /// and for Tier 3 compaction safety guards.
    live_snapshots: BTreeMap<Lsn, u32>,
}

impl CommitSequencer {
    /// Create a new sequencer.
    ///
    /// `start_lsn` lets the caller seed from the durable broker watermark so the
    /// commit-LSN and broker offset form one unified monotonic sequence.  Pass `0`
    /// for a fresh database or when seeding is not needed.
    ///
    /// # LSN layout
    ///
    /// Commit LSNs start at `start_lsn + 1` (the sentinel `start_lsn` represents
    /// "before any commit").  A read snapshot taken before the first commit sees
    /// `Lsn(start_lsn)`.  Any key committed at `Lsn(start_lsn + 1)` or later will
    /// conflict against that snapshot, which is exactly first-committer-wins.
    pub fn new(start_lsn: u64) -> Self {
        CommitSequencer {
            // Commits begin at start_lsn + 1 so that `Lsn(start_lsn)` is a
            // permanently valid "before any commit" sentinel that compares strictly
            // less than every commit LSN.  This means the very first commit at
            // Lsn(start_lsn + 1) satisfies `commit_lsn > snapshot_lsn` for any
            // snapshot taken before it (which has snapshot_lsn = start_lsn).
            next_lsn: start_lsn + 1,
            conflicts: HashMap::new(),
            live_snapshots: BTreeMap::new(),
        }
    }

    /// Register a read snapshot.
    ///
    /// Returns the LSN of the last committed transaction (`next_lsn - 1`), which
    /// is `Lsn(start_lsn)` if nothing has committed yet.  Increments the refcount
    /// for that LSN so [`gc`] knows it is still needed.
    ///
    /// The caller must pair every `register_snapshot` with exactly one
    /// [`release_snapshot`] on the same LSN to keep refcounts correct.
    pub fn register_snapshot(&mut self) -> Lsn {
        // next_lsn starts at start_lsn + 1, so next_lsn - 1 is always valid.
        let snap = Lsn(self.next_lsn - 1);
        *self.live_snapshots.entry(snap).or_insert(0) += 1;
        snap
    }

    /// Release a snapshot.
    ///
    /// Decrements the refcount for `s`; removes it from the map when the count
    /// reaches zero so [`gc`] may prune entries older than the next-oldest snapshot.
    pub fn release_snapshot(&mut self, s: Lsn) {
        if let Some(count) = self.live_snapshots.get_mut(&s) {
            if *count <= 1 {
                self.live_snapshots.remove(&s);
            } else {
                *count -= 1;
            }
        }
    }

    /// The oldest LSN for which at least one read snapshot is still open.
    ///
    /// Returns `Lsn(0)` when no snapshots are live.  Used by the compaction
    /// keep-set guard: any row version visible as of this LSN must not be GC'd.
    pub fn oldest_live_snapshot(&self) -> Lsn {
        self.live_snapshots
            .keys()
            .next()
            .copied()
            .unwrap_or(Lsn(0))
    }

    /// Attempt to commit a transaction's write-set (first-committer-wins, #83).
    ///
    /// For each key in `ws.keys`, checks whether it was last committed at a
    /// LSN strictly greater than `ws.snapshot_lsn`.  If ANY key conflicts,
    /// returns [`CommitOutcome::Conflict`] immediately (the caller must discard
    /// staged writes and retry).  On success, assigns the next monotonic LSN,
    /// records it for every key in the write-set, and returns
    /// [`CommitOutcome::Committed`].
    ///
    /// Pure opaque-key equality + integer compare — no schema, no model name,
    /// no field awareness.
    pub fn try_commit(&mut self, ws: &WriteSet) -> CommitOutcome {
        // Check all keys before touching next_lsn (all-or-nothing, no partial state).
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

    /// Prune conflict-map entries that no live snapshot can still conflict against.
    ///
    /// An entry `(key, commit_lsn)` is safe to drop when `commit_lsn <
    /// oldest_live_snapshot()`: no open snapshot predates `oldest`, so no future
    /// challenger will have a `snapshot_lsn < commit_lsn` for that entry.  Calling
    /// this periodically bounds the conflict map to O(keys modified since the oldest
    /// live snapshot), which is typically small under the single-writer Tier-2 model.
    pub fn gc(&mut self) {
        let oldest = self.oldest_live_snapshot();
        if oldest == Lsn(0) && self.live_snapshots.is_empty() {
            // No live snapshots at all: prune everything.
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

    /// First-committer-wins: A and B take snapshots before any commit.
    /// A commits key "row:0" first.  B's snapshot_lsn predates A's commit, so B conflicts.
    #[test]
    fn first_committer_wins_conflict() {
        let mut seq = CommitSequencer::new(0);
        // new(0) → next_lsn = 1; sentinel "before any commit" = Lsn(0).

        let snap_a = seq.register_snapshot(); // Lsn(0) — before any commit
        let snap_b = seq.register_snapshot(); // Lsn(0) — same, no commits yet

        // A commits key "row:0".
        let ws_a = WriteSet {
            keys: vec![key(b"row:0")],
            snapshot_lsn: snap_a,
        };
        let CommitOutcome::Committed(lsn_a) = seq.try_commit(&ws_a) else {
            panic!("A should commit");
        };
        assert_eq!(lsn_a, Lsn(1)); // first commit LSN

        // B tries the same key with snap_b = Lsn(0), but the key was committed at Lsn(1).
        // Lsn(1) > Lsn(0) → conflict.
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

    /// No conflict when the key was committed BEFORE the challenger's snapshot.
    #[test]
    fn no_conflict_when_key_committed_before_snapshot() {
        let mut seq = CommitSequencer::new(0);
        // next_lsn = 1, sentinel = Lsn(0).

        let snap_a = seq.register_snapshot(); // Lsn(0)

        // A commits key "row:0" → gets Lsn(1).
        let ws_a = WriteSet {
            keys: vec![key(b"row:0")],
            snapshot_lsn: snap_a,
        };
        let CommitOutcome::Committed(c) = seq.try_commit(&ws_a) else {
            panic!();
        };
        assert_eq!(c, Lsn(1));
        seq.release_snapshot(snap_a);

        // B takes a snapshot AFTER A committed → snap_b = Lsn(1).
        let snap_b = seq.register_snapshot(); // Lsn(1) = last committed
        assert_eq!(snap_b, Lsn(1));

        // A's key was committed at Lsn(1); B's snapshot_lsn = Lsn(1).
        // Condition: commit_lsn > snapshot_lsn → Lsn(1) > Lsn(1) = false → NO conflict.
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

    /// Snapshot refcount: register/release, oldest_live_snapshot correctness.
    #[test]
    fn snapshot_refcount_basic() {
        let mut seq = CommitSequencer::new(0);
        // next_lsn = 1, sentinel = Lsn(0).

        // No snapshots yet.
        assert_eq!(seq.oldest_live_snapshot(), Lsn(0));

        let s1 = seq.register_snapshot();
        assert_eq!(s1, Lsn(0)); // before any commit
        assert_eq!(seq.oldest_live_snapshot(), Lsn(0));

        // Commit something so next LSN advances.
        let ws = WriteSet {
            keys: vec![key(b"k")],
            snapshot_lsn: s1,
        };
        let CommitOutcome::Committed(c) = seq.try_commit(&ws) else {
            panic!();
        };
        assert_eq!(c, Lsn(1));

        let s2 = seq.register_snapshot();
        assert_eq!(s2, Lsn(1)); // last committed

        seq.release_snapshot(s1);
        // s2 still live at Lsn(1).
        assert_eq!(seq.oldest_live_snapshot(), Lsn(1));

        seq.release_snapshot(s2);
        // All released.
        assert!(seq.live_snapshots.is_empty());
    }

    /// Multiple live snapshots, oldest is correct.
    #[test]
    fn multiple_live_snapshots_oldest_correct() {
        let mut seq = CommitSequencer::new(0);
        // next_lsn = 1, sentinel = Lsn(0).

        let snap0 = seq.register_snapshot(); // Lsn(0) — before any commit
        assert_eq!(snap0, Lsn(0));

        let ws1 = WriteSet {
            keys: vec![key(b"a")],
            snapshot_lsn: snap0,
        };
        let CommitOutcome::Committed(c1) = seq.try_commit(&ws1) else {
            panic!()
        };
        assert_eq!(c1, Lsn(1));

        let snap1 = seq.register_snapshot(); // Lsn(1) = last committed
        assert_eq!(snap1, Lsn(1));

        let ws2 = WriteSet {
            keys: vec![key(b"b")],
            snapshot_lsn: snap1,
        };
        let CommitOutcome::Committed(c2) = seq.try_commit(&ws2) else {
            panic!()
        };
        assert_eq!(c2, Lsn(2));

        let snap2 = seq.register_snapshot(); // Lsn(2) = last committed
        assert_eq!(snap2, Lsn(2));

        // Three live snapshots: snap0=Lsn(0), snap1=Lsn(1), snap2=Lsn(2).
        assert_eq!(seq.oldest_live_snapshot(), Lsn(0));

        seq.release_snapshot(snap0);
        assert_eq!(seq.oldest_live_snapshot(), Lsn(1));

        seq.release_snapshot(snap1);
        assert_eq!(seq.oldest_live_snapshot(), Lsn(2));

        seq.release_snapshot(snap2);
        assert!(seq.live_snapshots.is_empty());
    }

    /// gc() bounds the conflict map.
    #[test]
    fn gc_prunes_old_entries() {
        let mut seq = CommitSequencer::new(0);
        // next_lsn = 1, sentinel = Lsn(0).

        // Commit several keys.
        let s0 = seq.register_snapshot(); // Lsn(0)
        for i in 0u8..5 {
            let ws = WriteSet {
                keys: vec![vec![i].into_boxed_slice()],
                snapshot_lsn: s0,
            };
            assert!(matches!(seq.try_commit(&ws), CommitOutcome::Committed(_)));
        }
        // 5 entries in conflict map (committed at Lsn(1)..=Lsn(5)).
        assert_eq!(seq.conflicts.len(), 5);

        // Release s0 (no more live snapshots).
        seq.release_snapshot(s0);

        // With no live snapshots, gc clears everything.
        seq.gc();
        assert!(seq.conflicts.is_empty(), "gc should prune all entries with no live snapshots");
    }

    /// gc() keeps entries needed by the oldest live snapshot.
    #[test]
    fn gc_keeps_entries_needed_by_snapshot() {
        let mut seq = CommitSequencer::new(0);
        // next_lsn = 1, sentinel = Lsn(0).

        let s_before = seq.register_snapshot(); // Lsn(0)

        // Commit key A → Lsn(1).
        let ws_a = WriteSet {
            keys: vec![key(b"A")],
            snapshot_lsn: s_before,
        };
        let CommitOutcome::Committed(ca) = seq.try_commit(&ws_a) else {
            panic!()
        };
        assert_eq!(ca, Lsn(1));

        // Take a snapshot after A commits.
        let s_after_a = seq.register_snapshot(); // Lsn(1)

        // Commit key B → Lsn(2).
        let ws_b = WriteSet {
            keys: vec![key(b"B")],
            snapshot_lsn: s_after_a,
        };
        let CommitOutcome::Committed(cb) = seq.try_commit(&ws_b) else {
            panic!()
        };
        assert_eq!(cb, Lsn(2));

        seq.release_snapshot(s_before);
        // s_after_a at Lsn(1) is still live.
        // oldest = Lsn(1). Keep entries with commit_lsn >= Lsn(1) → both A (Lsn(1)) and B (Lsn(2)).
        seq.gc();
        assert_eq!(seq.conflicts.len(), 2, "both entries still needed");

        seq.release_snapshot(s_after_a);
        // No live snapshots.
        seq.gc();
        assert!(seq.conflicts.is_empty());
    }
}
