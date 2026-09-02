use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use tokio::sync::broadcast;

use crate::ChangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncPolicy {
    Always,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedEvent {
    pub offset: u64,
    pub model: String,
    pub row_index: u64,
    pub kind: ChangeKind,
    pub bytes: Vec<u8>,
}

impl PersistedEvent {
    pub fn to_wire(&self) -> Vec<u8> {
        self.to_frame()
    }

    pub fn from_wire(buf: &[u8]) -> io::Result<PersistedEvent> {
        match Self::from_frame(buf)? {
            Some((event, _)) => Ok(event),
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete replication frame",
            )),
        }
    }

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
        let total_len = payload.len() + 4;

        let mut frame = Vec::with_capacity(4 + total_len);
        frame.extend_from_slice(&(total_len as u32).to_le_bytes());
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&checksum.to_le_bytes());
        frame
    }

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
            return Ok(None);
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

pub struct DurableBroker {
    path: PathBuf,
    file: File,
    fsync: FsyncPolicy,
    next_offset: u64,
    earliest: u64,
    sender: broadcast::Sender<PersistedEvent>,
}

impl DurableBroker {
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

        let (earliest, next_offset) = Self::scan_bounds(&path)?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;

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

        let _ = self.sender.send(event);
        Ok(offset)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    pub fn watermark(&self) -> u64 {
        self.next_offset - 1
    }

    pub fn earliest_retained(&self) -> u64 {
        self.earliest
    }

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
                None => break,
            }
        }
        Ok(out)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PersistedEvent> {
        self.sender.subscribe()
    }

    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

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

        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)?;
        self.earliest = retained.first().map(|e| e.offset).unwrap_or(self.next_offset);
        Ok(())
    }

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

pub struct CatchUp {
    pub replayed: Vec<PersistedEvent>,
    pub boundary: u64,
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
        assert_eq!(tail[0].bytes, vec![0xCC]);
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
        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(&(999u32).to_le_bytes()).unwrap();
            f.write_all(&[0xDE, 0xAD]).unwrap();
            f.sync_all().unwrap();
        }
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
        assert_eq!(b.watermark(), 5);
        let remaining = b.read_from(0, usize::MAX).unwrap();
        assert_eq!(remaining.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(b.record("M", 5, ChangeKind::Inserted, vec![]).unwrap(), 6);
    }

    #[tokio::test]
    async fn catch_up_from_stitches_replay_and_live_without_gap_or_dup() {
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        b.record("User", 0, ChangeKind::Inserted, vec![1]).unwrap();
        b.record("User", 1, ChangeKind::Inserted, vec![2]).unwrap();

        let mut catch_up = b.catch_up_from(1, usize::MAX).unwrap();
        assert_eq!(catch_up.replayed.iter().map(|e| e.offset).collect::<Vec<_>>(), vec![2]);
        assert_eq!(catch_up.boundary, 2);

        b.record("Post", 0, ChangeKind::Inserted, vec![3]).unwrap();
        b.record("Post", 1, ChangeKind::Inserted, vec![4]).unwrap();

        let mut applied: Vec<u64> = catch_up.replayed.iter().map(|e| e.offset).collect();
        while let Ok(ev) = catch_up.receiver.try_recv() {
            if ev.offset > catch_up.boundary {
                applied.push(ev.offset);
            }
        }
        assert_eq!(applied, vec![2, 3, 4]);
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
        let dir = tempdir().unwrap();
        let mut b = broker(dir.path());
        let junk = vec![0xFF, 0x00, 0xFE, 0x7F, 0x80];
        b.record("Anything", 42, ChangeKind::Updated, junk.clone()).unwrap();
        let got = b.read_from(0, 1).unwrap();
        assert_eq!(got[0].bytes, junk);
    }
}
