//! `forgedb-changefeed::durable` — a durable, offset-addressed, resumable change
//! broker (#82, realtime Direction C).
//!
//! Where [`crate::ChangeFeed`] is an in-process, best-effort *signal*
//! ([`tokio::sync::broadcast`] of `{model, row_index, kind}`, no durability),
//! [`DurableBroker`] is the substrate a cross-process / cross-network follower
//! resumes from. It records each committed change to a **CRC-framed,
//! append-only log** and assigns a **monotonic global offset**, so a subscriber
//! that persists the last offset it applied can reconnect and replay everything
//! after it — the resumable read-replica contract the WASM follower (#110)
//! needs.
//!
//! ## The red line holds — this crate still decodes nothing
//!
//! A [`PersistedEvent`] carries `model` (an **opaque routing tag**, exactly like
//! `forgedb-wal`'s model-name header), `row_index`, `kind`, `offset`, and
//! `bytes` — the **opaque committed row bytes**, stored and returned verbatim.
//! There is no field-typed member and no per-model branch anywhere in this
//! module: routing by model name and materialization of the typed record both
//! stay in generated code, precisely as with the in-process feed. The broker
//! moves opaque bytes tagged by an opaque name; that is all it will ever do.
//!
//! ## The offset *is* the ordering contract
//!
//! Offsets are a single monotonically increasing `u64` assigned in [`record`]
//! order. Because the generated server is a single writer, `record` order *is*
//! the commit order (the server-side `DatabaseSnapshot` boundary), so one global
//! offset sequences changes **across all models** without the broker ever
//! understanding a relation or a foreign key. A follower applies strictly in
//! offset order and is **idempotent by absolute offset** — replaying an already
//! applied offset is a no-op, the same discipline as WAL replay by absolute row
//! index. That idempotency is what makes "resume from the last persisted offset"
//! correct even if the last few in-flight events were re-sent.
//!
//! [`record`]: DurableBroker::record

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use tokio::sync::broadcast;

use crate::ChangeKind;

/// When the broker fsyncs its durable log.
///
/// Mirrors `forgedb-wal`'s policy so a server can configure both consistently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncPolicy {
    /// Fsync after every recorded event (maximum durability).
    Always,
    /// Never fsync automatically — the caller drives [`DurableBroker::flush`].
    Never,
}

/// A single durable, offset-addressed change record.
///
/// Field-blind by construction: `model` is an opaque routing tag and `bytes` are
/// opaque committed row bytes carried verbatim. This struct is the on-wire /
/// on-disk replication frame — it must never gain a field-typed member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedEvent {
    /// Monotonic global sequence assigned by the broker. `1`-based; `0` is the
    /// "cold" sentinel meaning "before the first event" (see
    /// [`DurableBroker::read_from`]).
    pub offset: u64,
    /// The model (or junction) name — an opaque routing tag, never decoded here.
    pub model: String,
    /// The append position of the row in that collection's storage.
    pub row_index: u64,
    /// The kind of change (insert / update / delete / link).
    pub kind: ChangeKind,
    /// The opaque committed row bytes. Stored and returned verbatim; the broker
    /// never interprets them.
    pub bytes: Vec<u8>,
}

impl PersistedEvent {
    /// Encode to the self-describing binary wire frame — **identical** to the
    /// durable on-disk framing, so the replication transport and the log share
    /// one codec. The transport sends exactly one frame per message; a follower
    /// decodes with [`from_wire`](PersistedEvent::from_wire).
    ///
    /// Field-blind: `bytes` are copied verbatim, never interpreted.
    pub fn to_wire(&self) -> Vec<u8> {
        self.to_frame()
    }

    /// Decode exactly one wire frame produced by [`to_wire`](PersistedEvent::to_wire).
    ///
    /// `Err` on a CRC mismatch, a structurally invalid frame, or a truncated /
    /// incomplete frame (a whole WS message must be one complete frame).
    pub fn from_wire(buf: &[u8]) -> io::Result<PersistedEvent> {
        match Self::from_frame(buf)? {
            Some((event, _)) => Ok(event),
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete replication frame",
            )),
        }
    }

    /// Serialize to the on-disk frame:
    /// `[4: total_len][payload][4: crc32]`, where
    /// `payload = [8: offset][8: row_index][1: kind][2: model_len][model][4: bytes_len][bytes]`.
    ///
    /// All integers little-endian. The framing (length prefix + trailing CRC32)
    /// mirrors `forgedb-wal` so torn-tail recovery is identical.
    fn to_frame(&self) -> Vec<u8> {
        let model = self.model.as_bytes();
        let mut payload = Vec::with_capacity(8 + 8 + 1 + 2 + model.len() + 4 + self.bytes.len());
        payload.extend_from_slice(&self.offset.to_le_bytes());
        payload.extend_from_slice(&self.row_index.to_le_bytes());
        payload.push(self.kind.to_byte());
        payload.extend_from_slice(&(model.len() as u16).to_le_bytes());
        payload.extend_from_slice(model);
        payload.extend_from_slice(&(self.bytes.len() as u32).to_le_bytes());
        payload.extend_from_slice(&self.bytes);

        let checksum = crc32fast::hash(&payload);
        let total_len = payload.len() + 4; // + trailing crc

        let mut frame = Vec::with_capacity(4 + total_len);
        frame.extend_from_slice(&(total_len as u32).to_le_bytes());
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&checksum.to_le_bytes());
        frame
    }

    /// Parse one frame from the front of `buf`.
    ///
    /// Returns `Ok(Some((event, consumed)))` on a complete, CRC-valid frame,
    /// `Ok(None)` on a torn tail (incomplete final frame — stop and keep the
    /// valid prefix), and `Err` on a CRC mismatch or structurally invalid frame.
    fn from_frame(buf: &[u8]) -> io::Result<Option<(PersistedEvent, usize)>> {
        if buf.len() < 4 {
            return Ok(None);
        }
        let total_len = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
        if total_len < 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame too short to contain a checksum",
            ));
        }
        if buf.len() < 4 + total_len {
            return Ok(None); // torn tail
        }
        let body = &buf[4..4 + total_len];
        let payload = &body[..body.len() - 4];
        let stored = u32::from_le_bytes(body[body.len() - 4..].try_into().unwrap());
        if crc32fast::hash(payload) != stored {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "checksum mismatch — broker log corrupted",
            ));
        }

        // Decode payload fields (positional; never interprets `bytes`).
        let mut p = 0usize;
        let need = |p: usize, n: usize| -> io::Result<()> {
            if payload.len() < p + n {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated broker frame payload",
                ))
            } else {
                Ok(())
            }
        };
        need(p, 8)?;
        let offset = u64::from_le_bytes(payload[p..p + 8].try_into().unwrap());
        p += 8;
        need(p, 8)?;
        let row_index = u64::from_le_bytes(payload[p..p + 8].try_into().unwrap());
        p += 8;
        need(p, 1)?;
        let kind = ChangeKind::from_byte(payload[p]).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "unknown ChangeKind byte")
        })?;
        p += 1;
        need(p, 2)?;
        let model_len = u16::from_le_bytes(payload[p..p + 2].try_into().unwrap()) as usize;
        p += 2;
        need(p, model_len)?;
        let model = String::from_utf8(payload[p..p + model_len].to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        p += model_len;
        need(p, 4)?;
        let bytes_len = u32::from_le_bytes(payload[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        need(p, bytes_len)?;
        let bytes = payload[p..p + bytes_len].to_vec();

        Ok(Some((
            PersistedEvent {
                offset,
                model,
                row_index,
                kind,
                bytes,
            },
            4 + total_len,
        )))
    }
}

/// A durable, offset-addressed, resumable broker.
///
/// Records committed changes to an append-only, CRC-framed log and fans the same
/// events out to live [`subscribe`](DurableBroker::subscribe) receivers. A
/// follower catches up by [`read_from`](DurableBroker::read_from) its last
/// persisted offset, then attaches a live subscription — see
/// [`catch_up_from`](DurableBroker::catch_up_from) for the race-free stitch.
///
/// **Single-writer.** [`record`](DurableBroker::record) takes `&mut self`; the
/// generated server owns the one writer (v1 single-writer-per-process), so
/// `record` order is commit order and offsets are a faithful global ordering.
/// Read methods take `&self`.
pub struct DurableBroker {
    path: PathBuf,
    file: File,
    fsync: FsyncPolicy,
    /// Offset the next recorded event will receive. `1`-based.
    next_offset: u64,
    /// Lowest offset still present in the log (after any [`prune_through`]).
    /// Equal to `next_offset` when the log is empty.
    ///
    /// [`prune_through`]: DurableBroker::prune_through
    earliest: u64,
    sender: broadcast::Sender<PersistedEvent>,
}

impl DurableBroker {
    /// Open or create a broker whose durable log lives at `path`.
    ///
    /// An existing log is scanned torn-tail-safe to recover the current
    /// watermark and earliest retained offset, so offsets stay monotonic across
    /// restarts. `capacity` bounds each live subscriber's in-memory ring (lag
    /// past it drops the oldest live events — a lagging follower falls back to
    /// durable replay, which is the whole point).
    pub fn open<P: AsRef<Path>>(
        path: P,
        fsync: FsyncPolicy,
        capacity: usize,
    ) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        // Recover watermark + earliest from any existing log (torn-tail safe).
        let (earliest, next_offset) = Self::scan_bounds(&path)?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;

        // If the on-disk log had a torn tail, truncate it back to the last
        // valid frame so future appends never sit behind garbage.
        let valid_len = Self::valid_prefix_len(&path)?;
        {
            let f = OpenOptions::new().write(true).open(&path)?;
            f.set_len(valid_len)?;
            f.sync_all()?;
        }

        let (sender, _rx) = broadcast::channel(capacity.max(1));
        Ok(DurableBroker {
            path,
            file,
            fsync,
            next_offset,
            earliest,
            sender,
        })
    }

    /// Record a committed change, assigning the next global offset.
    ///
    /// Durably appends the frame (per the [`FsyncPolicy`]) **before** fanning it
    /// out to live subscribers, then returns the assigned offset. `model` is an
    /// opaque tag and `bytes` are opaque committed row bytes — neither is
    /// interpreted.
    pub fn record(
        &mut self,
        model: &str,
        row_index: u64,
        kind: ChangeKind,
        bytes: Vec<u8>,
    ) -> io::Result<u64> {
        let offset = self.next_offset;
        let event = PersistedEvent {
            offset,
            model: model.to_string(),
            row_index,
            kind,
            bytes,
        };

        self.file.write_all(&event.to_frame())?;
        if self.fsync == FsyncPolicy::Always {
            self.file.sync_all()?;
        }

        self.next_offset += 1;
        if self.earliest == 0 {
            self.earliest = offset;
        }

        // Best-effort live fan-out (0 subscribers is not an error).
        let _ = self.sender.send(event);
        Ok(offset)
    }

    /// Flush buffered log bytes to disk. Needed only under [`FsyncPolicy::Never`].
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    /// The highest offset assigned so far (`0` if nothing has been recorded).
    /// A follower that has applied through this offset is fully caught up.
    pub fn watermark(&self) -> u64 {
        self.next_offset - 1
    }

    /// The lowest offset still retained in the durable log.
    ///
    /// A follower whose last-applied offset is **below** `earliest_retained() - 1`
    /// has fallen off the retained tail and must re-baseline from a full snapshot
    /// (`forgedb-backup`) before resuming — this is the snapshot-vs-tail cutover
    /// point. Equals [`watermark`](DurableBroker::watermark)` + 1` when the log
    /// is empty.
    pub fn earliest_retained(&self) -> u64 {
        self.earliest
    }

    /// Read persisted events with `offset > after`, up to `max` of them, in
    /// offset order.
    ///
    /// A cold follower passes `after = 0` to read from the beginning of the
    /// retained log. Returns fewer than `max` (possibly zero) when the tail is
    /// reached. Scans the log from the front — O(retained) per call, acceptable
    /// for the infrequent catch-up path at v1's application-dataset scale.
    pub fn read_from(&self, after: u64, max: usize) -> io::Result<Vec<PersistedEvent>> {
        let mut out = Vec::new();
        if max == 0 {
            return Ok(out);
        }
        let mut f = File::open(&self.path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;

        let mut pos = 0usize;
        while pos < buf.len() {
            match PersistedEvent::from_frame(&buf[pos..])? {
                Some((event, consumed)) => {
                    pos += consumed;
                    if event.offset > after {
                        out.push(event);
                        if out.len() >= max {
                            break;
                        }
                    }
                }
                None => break, // torn tail — stop at the valid prefix
            }
        }
        Ok(out)
    }

    /// Subscribe to the live tail. The receiver observes every event recorded
    /// *after* this call. Combine with [`read_from`](DurableBroker::read_from)
    /// via [`catch_up_from`](DurableBroker::catch_up_from) for a gap-free resume.
    pub fn subscribe(&self) -> broadcast::Receiver<PersistedEvent> {
        self.sender.subscribe()
    }

    /// The number of live subscribers currently attached.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Race-free resume: subscribe to the live tail, then replay everything
    /// durably retained after `after` up to the current watermark.
    ///
    /// Returns a [`CatchUp`] carrying the replayed events, the `boundary` offset
    /// they were replayed through, and a live `receiver`. The caller applies
    /// `replayed` first, then drains `receiver` **skipping any event whose
    /// `offset <= boundary`** — those were already covered by the replay, and
    /// skipping them is exactly the "idempotent by absolute offset" rule. No
    /// event in `(after, ∞)` is dropped: the receiver was subscribed before the
    /// boundary was read, so any offset `> boundary` is guaranteed to arrive
    /// live.
    ///
    /// Callers hold the single-writer discipline (this is `&self`; `record` is
    /// `&mut self`), so no `record` interleaves the subscribe/replay pair.
    pub fn catch_up_from(&self, after: u64, max: usize) -> io::Result<CatchUp> {
        let receiver = self.sender.subscribe();
        let boundary = self.watermark();
        let replayed = self.read_from(after, max)?;
        Ok(CatchUp {
            replayed,
            boundary,
            receiver,
        })
    }

    /// Drop every event with `offset <= through`, rewriting the log to retain
    /// only newer events. Advances [`earliest_retained`]; followers still below
    /// the new earliest must re-baseline from a snapshot.
    ///
    /// Retention policy (when to prune) is the caller's — typically after a base
    /// snapshot has advanced past `through`, or on a size bound. Rewrites via a
    /// temp file + rename so a crash mid-prune leaves the old log intact.
    ///
    /// [`earliest_retained`]: DurableBroker::earliest_retained
    pub fn prune_through(&mut self, through: u64) -> io::Result<()> {
        let retained = self.read_from(through, usize::MAX)?;

        let tmp = self.path.with_extension("log.compacting");
        {
            let mut out = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)?;
            for event in &retained {
                out.write_all(&event.to_frame())?;
            }
            out.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;

        // Reopen the append handle against the rewritten log.
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)?;
        self.earliest = retained.first().map(|e| e.offset).unwrap_or(self.next_offset);
        Ok(())
    }

    /// Scan an existing log for `(earliest_offset, next_offset)` without holding
    /// the file open. Torn-tail safe: stops at the first incomplete frame.
    fn scan_bounds(path: &Path) -> io::Result<(u64, u64)> {
        let mut f = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((0, 1)),
            Err(e) => return Err(e),
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;

        let mut pos = 0usize;
        let mut earliest = 0u64;
        let mut max_offset = 0u64;
        while pos < buf.len() {
            match PersistedEvent::from_frame(&buf[pos..])? {
                Some((event, consumed)) => {
                    pos += consumed;
                    if earliest == 0 {
                        earliest = event.offset;
                    }
                    max_offset = event.offset;
                }
                None => break,
            }
        }
        let next_offset = max_offset + 1;
        let earliest = if earliest == 0 { next_offset } else { earliest };
        Ok((earliest, next_offset))
    }

    /// Byte length of the log's valid (CRC-clean, complete) prefix — everything
    /// up to a torn tail. Used to truncate a torn tail on open.
    fn valid_prefix_len(path: &Path) -> io::Result<u64> {
        let mut f = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        let mut pos = 0usize;
        while pos < buf.len() {
            match PersistedEvent::from_frame(&buf[pos..])? {
                Some((_, consumed)) => pos += consumed,
                None => break,
            }
        }
        f.seek(SeekFrom::Start(0))?;
        Ok(pos as u64)
    }
}

/// The result of [`DurableBroker::catch_up_from`]: durable replay + a live
/// receiver, stitched gap-free.
///
/// Apply [`replayed`](CatchUp::replayed) in order, then drain
/// [`receiver`](CatchUp::receiver) ignoring any event whose `offset <=`
/// [`boundary`](CatchUp::boundary).
pub struct CatchUp {
    /// Durably retained events in `(after, boundary]`, in offset order.
    pub replayed: Vec<PersistedEvent>,
    /// The watermark the replay was taken through; live events at or below it
    /// are duplicates of `replayed` and must be skipped.
    pub boundary: u64,
    /// The live tail, subscribed *before* `boundary` was read.
    pub receiver: broadcast::Receiver<PersistedEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn broker(dir: &std::path::Path) -> DurableBroker {
        DurableBroker::open(dir.join("broker.log"), FsyncPolicy::Always, 64).unwrap()
    }

    #[test]
    fn assigns_monotonic_offsets_from_one() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        assert_eq!(b.watermark(), 0);
        assert_eq!(b.record("User", 0, ChangeKind::Inserted, vec![1, 2, 3]).unwrap(), 1);
        assert_eq!(b.record("Post", 5, ChangeKind::Updated, vec![]).unwrap(), 2);
        assert_eq!(b.record("User", 1, ChangeKind::Deleted, vec![9]).unwrap(), 3);
        assert_eq!(b.watermark(), 3);
        assert_eq!(b.earliest_retained(), 1);
    }

    #[test]
    fn read_from_returns_events_after_offset_verbatim() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        b.record("User", 0, ChangeKind::Inserted, vec![0xAA, 0xBB]).unwrap();
        b.record("User", 1, ChangeKind::Inserted, vec![0xCC]).unwrap();
        b.record("Post", 0, ChangeKind::Inserted, vec![]).unwrap();

        let tail = b.read_from(1, 10).unwrap();
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].offset, 2);
        assert_eq!(tail[0].model, "User");
        assert_eq!(tail[0].row_index, 1);
        assert_eq!(tail[0].bytes, vec![0xCC]); // opaque bytes round-trip verbatim
        assert_eq!(tail[1].offset, 3);
        assert_eq!(tail[1].model, "Post");
        assert_eq!(tail[1].kind, ChangeKind::Inserted);
    }

    #[test]
    fn read_from_respects_max() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        for i in 0..5 {
            b.record("M", i, ChangeKind::Inserted, vec![i as u8]).unwrap();
        }
        let two = b.read_from(0, 2).unwrap();
        assert_eq!(two.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn cold_follower_reads_from_zero() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        b.record("User", 0, ChangeKind::Inserted, vec![1]).unwrap();
        let all = b.read_from(0, usize::MAX).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].offset, 1);
    }

    #[test]
    fn survives_reopen_offsets_stay_monotonic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("broker.log");
        {
            let mut b = DurableBroker::open(&path, FsyncPolicy::Always, 64).unwrap();
            b.record("User", 0, ChangeKind::Inserted, vec![1]).unwrap();
            b.record("User", 1, ChangeKind::Inserted, vec![2]).unwrap();
        }
        // Reopen: watermark recovered, next offset continues, old events replayable.
        let mut b = DurableBroker::open(&path, FsyncPolicy::Always, 64).unwrap();
        assert_eq!(b.watermark(), 2);
        assert_eq!(b.earliest_retained(), 1);
        assert_eq!(b.record("Post", 0, ChangeKind::Inserted, vec![3]).unwrap(), 3);
        let all = b.read_from(0, usize::MAX).unwrap();
        assert_eq!(all.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn torn_tail_is_recovered_to_valid_prefix() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("broker.log");
        {
            let mut b = DurableBroker::open(&path, FsyncPolicy::Always, 64).unwrap();
            b.record("User", 0, ChangeKind::Inserted, vec![1]).unwrap();
            b.record("User", 1, ChangeKind::Inserted, vec![2]).unwrap();
        }
        // Corrupt: append a half-written frame (a length prefix promising bytes
        // that never arrive).
        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(&(999u32).to_le_bytes()).unwrap();
            f.write_all(&[0xDE, 0xAD]).unwrap();
            f.sync_all().unwrap();
        }
        // Reopen truncates the torn tail; the two valid events survive and the
        // next append lands at offset 3.
        let mut b = DurableBroker::open(&path, FsyncPolicy::Always, 64).unwrap();
        assert_eq!(b.watermark(), 2);
        assert_eq!(b.record("User", 2, ChangeKind::Inserted, vec![3]).unwrap(), 3);
        let all = b.read_from(0, usize::MAX).unwrap();
        assert_eq!(all.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn prune_through_advances_earliest_and_drops_old() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        for i in 0..5 {
            b.record("M", i, ChangeKind::Inserted, vec![i as u8]).unwrap();
        }
        b.prune_through(2).unwrap();
        assert_eq!(b.earliest_retained(), 3);
        assert_eq!(b.watermark(), 5); // watermark unaffected
        let remaining = b.read_from(0, usize::MAX).unwrap();
        assert_eq!(remaining.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![3, 4, 5]);
        // Recording continues monotonically after a prune.
        assert_eq!(b.record("M", 5, ChangeKind::Inserted, vec![]).unwrap(), 6);
    }

    #[tokio::test]
    async fn catch_up_from_stitches_replay_and_live_without_gap_or_dup() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        // Pre-existing durable history.
        b.record("User", 0, ChangeKind::Inserted, vec![1]).unwrap();
        b.record("User", 1, ChangeKind::Inserted, vec![2]).unwrap();

        // Follower resumes from offset 1: should replay offset 2, then get live.
        let mut catch_up = b.catch_up_from(1, usize::MAX).unwrap();
        assert_eq!(catch_up.replayed.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![2]);
        assert_eq!(catch_up.boundary, 2);

        // New live events after the boundary.
        b.record("Post", 0, ChangeKind::Inserted, vec![3]).unwrap();
        b.record("Post", 1, ChangeKind::Inserted, vec![4]).unwrap();

        // Drain live, applying the idempotency rule (skip offset <= boundary).
        let mut applied: Vec<u64> = catch_up.replayed.iter().map(|e| e.offset).collect();
        while let Ok(ev) = catch_up.receiver.try_recv() {
            if ev.offset > catch_up.boundary {
                applied.push(ev.offset);
            }
        }
        assert_eq!(applied, vec![2, 3, 4]); // no gap, no duplicate
    }

    #[tokio::test]
    async fn live_subscribers_receive_recorded_events() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        let mut rx = b.subscribe();
        b.record("User", 7, ChangeKind::Inserted, vec![0xAB]).unwrap();
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.offset, 1);
        assert_eq!(ev.model, "User");
        assert_eq!(ev.row_index, 7);
        assert_eq!(ev.bytes, vec![0xAB]);
    }

    #[test]
    fn wire_frame_round_trips_verbatim() {
        let ev = PersistedEvent {
            offset: 7,
            model: "User".to_string(),
            row_index: 42,
            kind: ChangeKind::Deleted,
            bytes: vec![0xFF, 0x00, 0x10],
        };
        let wire = ev.to_wire();
        let back = PersistedEvent::from_wire(&wire).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn from_wire_rejects_truncated_frame() {
        let ev = PersistedEvent {
            offset: 1,
            model: "M".to_string(),
            row_index: 0,
            kind: ChangeKind::Inserted,
            bytes: vec![1, 2, 3],
        };
        let wire = ev.to_wire();
        assert!(PersistedEvent::from_wire(&wire[..wire.len() - 2]).is_err());
    }

    #[test]
    fn opaque_bytes_are_never_interpreted() {
        // Bytes that are not valid UTF-8 and look nothing like a field round-trip
        // verbatim — proving the broker treats them as opaque.
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        let junk = vec![0xFF, 0x00, 0xFE, 0x7F, 0x80];
        b.record("Anything", 42, ChangeKind::Updated, junk.clone()).unwrap();
        let got = b.read_from(0, 1).unwrap();
        assert_eq!(got[0].bytes, junk);
    }
}
