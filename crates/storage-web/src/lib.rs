//! `forgedb-storage-web` — the browser read-replica storage backend (#110).
//!
//! An in-memory-arena columnar backend with **byte-identical positional
//! semantics** to `forgedb-storage-native`. Selected by the `forgedb-storage`
//! facade on `wasm32`; the generated `database.rs` links it unchanged.
//!
//! The load-bearing observation: positional
//! I/O over an in-memory byte buffer is naturally synchronous, so the per-row
//! column API (`append_uuid`, `read_string(index)`, …) stays sync and unchanged.
//! Async is quarantined to the [`hydrate`]/[`dump`] boundary — loading column
//! blobs from IndexedDB/OPFS on open and flushing them back on commit.
//!
//! This crate is **schema-agnostic substrate**: it moves opaque path→bytes
//! blobs and knows no model, field, or relation. It also builds on host targets
//! (it uses no `web-sys`), which is what lets its arena semantics be unit-tested
//! natively against the native engine's contract.

mod columns;
mod dir_lock;
mod manifest;
mod snapshot;
pub mod store;

// Browser persistence glue (the async hydrate/commit boundary). wasm32 only —
// it links `web-sys`/IndexedDB/OPFS. The arena core above is target-agnostic.
#[cfg(target_arch = "wasm32")]
pub mod persist;

pub use columns::{
    BufferedFixedColumn, BufferedVariableColumn, ColumnExport, FixedColumn, FixedColumnReader,
    Tombstones, TombstonesReader, VariableColumn, VariableColumnReader,
};
pub use dir_lock::DirLock;
pub use manifest::{ColumnKind, ColumnMetadata, ColumnType, Manifest, RowAnchor};
pub use snapshot::Snapshot;

// The async boundary the persistence glue / transport calls.
pub use store::{DirtyColumn, LazySource, dump, hydrate};

// WAL re-exports — mirror the native crate's surface. On wasm the WAL is the
// in-memory implementation from `forgedb-wal` (the follower keeps no file WAL).
pub use forgedb_wal::{FsyncPolicy, WalEntry, WalManager, WalOperation};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn fixed_column_roundtrips_like_the_file_engine() {
        store::clear();
        let mut c = FixedColumn::new(p("m/fixed/u64_0.bin"), 8).unwrap();
        assert!(c.is_empty());
        c.append_u64(1001).unwrap();
        c.append_u64(42).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c.read_u64(0).unwrap(), 1001);
        assert_eq!(c.read_u64(1).unwrap(), 42);
        assert!(c.read_u64(2).is_err()); // out of bounds
    }

    #[test]
    fn fixed_column_all_widths() {
        store::clear();
        let mut u = FixedColumn::new(p("w/u32.bin"), 4).unwrap();
        u.append_u32(7).unwrap();
        assert_eq!(u.read_u32(0).unwrap(), 7);

        let mut b = FixedColumn::new(p("w/bool.bin"), 1).unwrap();
        b.append_bool(true).unwrap();
        b.append_bool(false).unwrap();
        assert!(b.read_bool(0).unwrap());
        assert!(!b.read_bool(1).unwrap());

        let mut id = FixedColumn::new(p("w/uuid.bin"), 16).unwrap();
        let val = [9u8; 16];
        id.append_uuid(val).unwrap();
        assert_eq!(id.read_uuid(0).unwrap(), val);

        let mut fb = FixedColumn::new(p("w/fb.bin"), 3).unwrap();
        assert!(fb.append_bytes(&[1, 2]).is_err()); // wrong width
        fb.append_bytes(&[1, 2, 3]).unwrap();
        assert_eq!(fb.read_bytes(0).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn variable_column_roundtrips_and_truncates() {
        store::clear();
        let mut v = VariableColumn::new(p("m/var/s_data.bin"), p("m/var/s_off.bin")).unwrap();
        v.append_string("hello").unwrap();
        v.append_string("").unwrap();
        v.append_string("world!").unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v.read_string(0).unwrap(), "hello");
        assert_eq!(v.read_string(1).unwrap(), "");
        assert_eq!(v.read_string(2).unwrap(), "world!");
        v.truncate_to_rows(1).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v.read_string(0).unwrap(), "hello");
        assert!(v.read_string(1).is_err());
    }

    #[test]
    fn buffered_variable_read_str_matches_read_string() {
        // #224: the browser twin of the native borrowed read. Generated code calls
        // `read_str` on both targets, so the web backend must carry the same name,
        // the same slot addressing, and the same error kinds.
        store::clear();
        let mut v = VariableColumn::new(p("bs/var/s_data.bin"), p("bs/var/s_off.bin")).unwrap();
        for s in ["", "a", "hello world", "unicode ✓ é"] {
            v.append_string(s).unwrap();
        }
        let buf = v.gather_buffered(&[3, 0, 2, 1]).unwrap();
        for slot in 0..4usize {
            assert_eq!(buf.read_str(slot).unwrap(), buf.read_string(slot).unwrap());
        }
        assert_eq!(buf.read_str(0).unwrap(), "unicode ✓ é");
        assert_eq!(buf.read_str(1).unwrap(), "");
        assert_eq!(
            buf.read_str(4).unwrap_err().kind(),
            buf.read_string(4).unwrap_err().kind()
        );
    }

    /// #231: the arena `append_tagged` must lay bytes down exactly as the
    /// concatenation it replaces, or a replica's arenas diverge from the
    /// server's files it is following. Same corpus the native guard uses.
    #[test]
    fn append_tagged_is_byte_identical_to_the_concatenation() {
        store::clear();
        let mut new_col = VariableColumn::new(p("t/new_d.bin"), p("t/new_o.bin")).unwrap();
        let mut old_col = VariableColumn::new(p("t/old_d.bin"), p("t/old_o.bin")).unwrap();

        for value in ["", "a", "hello world", "unicode ✓ é 日本語", "embedded\u{0}nul"] {
            new_col.append_tagged(1, value).unwrap();
            new_col.append_tagged(0, "").unwrap();

            let mut encoded = String::with_capacity(value.len() + 1);
            encoded.push('\u{1}');
            encoded.push_str(value);
            old_col.append_string(&encoded).unwrap();
            old_col.append_string(&String::from('\u{0}')).unwrap();
        }

        let bytes = |path: &str| {
            store::with_bytes(&p(path), |b| Ok::<Vec<u8>, std::io::Error>(b.to_vec())).unwrap()
        };
        assert_eq!(bytes("t/new_d.bin"), bytes("t/old_d.bin"), "data arena");
        assert_eq!(bytes("t/new_o.bin"), bytes("t/old_o.bin"), "offsets arena");

        // And it reads back through both readers, including the borrowed one.
        assert_eq!(new_col.read_string(0).unwrap(), "\u{1}");
        assert_eq!(new_col.read_string(1).unwrap(), "\u{0}");
        assert_eq!(new_col.read_string(4).unwrap(), "\u{1}hello world");
        let indices: Vec<usize> = (0..new_col.len()).collect();
        let buf = new_col.gather_buffered(&indices).unwrap();
        for slot in 0..indices.len() {
            assert_eq!(buf.read_str(slot).unwrap(), buf.read_string(slot).unwrap());
        }
    }

    #[test]
    fn fixed_column_gather_matches_native_semantics() {
        store::clear();
        let mut c = FixedColumn::new(p("g/u64.bin"), 8).unwrap();
        for v in [10u64, 11, 12, 13, 14] {
            c.append_u64(v).unwrap();
        }

        // Reordered non-contiguous subset — the gather (non-alias) path.
        let out = c.gather(&[3, 0, 4]).unwrap();
        assert_eq!(out.len(), 3 * 8);
        assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 13);
        assert_eq!(u64::from_le_bytes(out[8..16].try_into().unwrap()), 10);
        assert_eq!(u64::from_le_bytes(out[16..24].try_into().unwrap()), 14);

        assert!(c.gather(&[]).unwrap().is_empty()); // empty selection
        assert!(c.gather(&[0, 5]).is_err()); // out of bounds
    }

    #[test]
    fn tombstones_roundtrip() {
        store::clear();
        let mut t = Tombstones::new(p("m/tomb.bin")).unwrap();
        t.append(false).unwrap();
        t.append(true).unwrap();
        assert_eq!(t.len(), 2);
        assert!(!t.is_deleted(0).unwrap());
        assert!(t.is_deleted(1).unwrap());
    }

    #[test]
    fn reader_sees_writer_appends_live() {
        store::clear();
        let mut c = FixedColumn::new(p("r/u32.bin"), 4).unwrap();
        let reader = c.reader().unwrap();
        assert_eq!(reader.len(), 0);
        c.append_u32(5).unwrap();
        // Reader created before the append still observes it (live length).
        assert_eq!(reader.len(), 1);
        assert_eq!(reader.read_u32(0).unwrap(), 5);
    }

    #[test]
    fn hydrate_then_read_and_dump_roundtrips() {
        store::clear();
        // Simulate the open boundary: blobs loaded from IndexedDB/OPFS.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&123u64.to_le_bytes());
        store::hydrate([(p("h/u64.bin"), bytes)]);
        let c = FixedColumn::new(p("h/u64.bin"), 8).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c.read_u64(0).unwrap(), 123);
        // Commit boundary: snapshot arenas for persistence.
        let dumped = store::dump();
        assert!(dumped.iter().any(|(k, _)| k == &p("h/u64.bin")));
    }

    #[test]
    fn manifest_saves_and_loads_through_arena() {
        store::clear();
        let m = Manifest {
            schema_version: 1,
            row_count: 3,
            columns: vec![ColumnMetadata {
                name: "id".into(),
                column_type: ColumnType::U64,
                column_index: 0,
                value_size: 8,
                kind: ColumnKind::Fixed,
                relative_path: "fixed/u64_0.bin".into(),
            }],
            wal_enabled: false,
            last_checkpoint: 0,
            compaction_epoch: 0,
            format_version: 1,
            row_anchor: Some(RowAnchor {
                relative_path: "tombstones.bin".into(),
                bytes_per_row: 1,
            }),
            auto_sequences: Default::default(),
        };
        m.save_to(&p("m/manifest.json")).unwrap();
        let back = Manifest::load_from(&p("m/manifest.json")).unwrap();
        assert_eq!(back.row_count, 3);
        assert_eq!(back.columns.len(), 1);
        assert_eq!(back.row_anchor.unwrap().bytes_per_row, 1);
    }

    #[test]
    fn dir_lock_always_acquires() {
        assert!(DirLock::acquire(&p("anywhere")).is_ok());
    }

    #[test]
    fn snapshot_visibility() {
        let s = Snapshot::new(2);
        assert!(s.visible(0));
        assert!(s.visible(1));
        assert!(!s.visible(2));
        assert_eq!(s.watermark(), 2);
    }

    // ---- lazy partial hydrate (the OPFS sync-fault-in mechanism, #110 #2) ----

    use std::collections::HashMap;

    /// A synchronous [`LazySource`] over an in-memory blob map — the native
    /// stand-in for the OPFS sync-access-handle map, so the fault-in mechanism
    /// is unit-tested without a browser. Records which paths were actually read.
    struct MapSource {
        blobs: HashMap<PathBuf, Vec<u8>>,
        reads: std::rc::Rc<std::cell::RefCell<Vec<PathBuf>>>,
    }
    impl store::LazySource for MapSource {
        fn len(&self, path: &std::path::Path) -> Option<usize> {
            self.blobs.get(path).map(Vec::len)
        }
        fn read(&self, path: &std::path::Path) -> Option<Vec<u8>> {
            let v = self.blobs.get(path)?.clone();
            self.reads.borrow_mut().push(path.to_path_buf());
            Some(v)
        }
    }

    #[test]
    fn lazy_source_answers_len_without_reading_then_faults_in_on_read() {
        store::clear();
        let reads = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut blobs = HashMap::new();
        // Two u64 rows persisted for one column; a second column also persisted.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7u64.to_le_bytes());
        bytes.extend_from_slice(&9u64.to_le_bytes());
        blobs.insert(p("db/a/u64_0.bin"), bytes);
        blobs.insert(p("db/b/u64_0.bin"), 5u64.to_le_bytes().to_vec());
        store::set_source(Box::new(MapSource {
            blobs,
            reads: reads.clone(),
        }));

        // Constructing the column must NOT read bytes (partial hydrate); its
        // length is answered from the source's on-disk size.
        let a = FixedColumn::new(p("db/a/u64_0.bin"), 8).unwrap();
        assert_eq!(a.len(), 2, "len from source, no read");
        assert!(reads.borrow().is_empty(), "len must not fault in");

        // First real read faults the whole column in — once.
        assert_eq!(a.read_u64(0).unwrap(), 7);
        assert_eq!(a.read_u64(1).unwrap(), 9);
        assert_eq!(&*reads.borrow(), &[p("db/a/u64_0.bin")]);

        // The untouched second column was never read — the point of partial
        // hydrate.
        assert_eq!(reads.borrow().len(), 1);
        let _b = FixedColumn::new(p("db/b/u64_0.bin"), 8).unwrap();
        assert_eq!(reads.borrow().len(), 1, "constructing b does not read it");
    }

    #[test]
    fn append_faults_in_before_growing_preserving_row_alignment() {
        store::clear();
        let reads = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut blobs = HashMap::new();
        blobs.insert(p("db/c/u32_0.bin"), 11u32.to_le_bytes().to_vec());
        store::set_source(Box::new(MapSource {
            blobs,
            reads: reads.clone(),
        }));

        // A column with one persisted row; appending must fault the existing row
        // in first, so the new row lands at index 1 (never onto a phantom-empty
        // arena — the alignment hazard the PM flagged).
        let mut c = FixedColumn::new(p("db/c/u32_0.bin"), 4).unwrap();
        c.append_u32(22).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c.read_u32(0).unwrap(), 11);
        assert_eq!(c.read_u32(1).unwrap(), 22);
    }

    #[test]
    fn dirty_columns_emits_only_grown_tails_and_meta_rewrites() {
        store::clear();
        let reads = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut blobs = HashMap::new();
        blobs.insert(p("db/d/u32_0.bin"), 1u32.to_le_bytes().to_vec()); // 4 bytes durable
        store::set_source(Box::new(MapSource {
            blobs,
            reads: reads.clone(),
        }));

        // Load + append one row to the persisted column.
        let mut d = FixedColumn::new(p("db/d/u32_0.bin"), 4).unwrap();
        d.append_u32(2).unwrap();
        // A brand-new column (no source backing).
        let mut e = FixedColumn::new(p("db/e/u32_0.bin"), 4).unwrap();
        e.append_u32(3).unwrap();
        // A metadata blob (watermark-like).
        store::put(p("db/_watermark"), 42u64.to_le_bytes().to_vec());

        let mut dirty = store::dirty_columns();
        dirty.sort_by(|a, b| a.path.cmp(&b.path));

        let by = |name: &str| {
            dirty
                .iter()
                .find(|dc| dc.path == p(name))
                .expect("present in dirty set")
        };
        // Loaded+appended: only the 4-byte tail past the committed prefix.
        let d_dc = by("db/d/u32_0.bin");
        assert_eq!(d_dc.offset, 4);
        assert_eq!(d_dc.bytes, 2u32.to_le_bytes().to_vec());
        assert!(!d_dc.truncate);
        // New column: whole content from offset 0 (committed len defaults 0).
        let e_dc = by("db/e/u32_0.bin");
        assert_eq!(e_dc.offset, 0);
        assert_eq!(e_dc.bytes, 3u32.to_le_bytes().to_vec());
        // Metadata: whole rewrite.
        let w_dc = by("db/_watermark");
        assert!(w_dc.truncate);
        assert_eq!(w_dc.offset, 0);

        // After marking committed, an unchanged column is no longer dirty.
        store::mark_committed(&p("db/d/u32_0.bin"), 8);
        store::mark_committed(&p("db/e/u32_0.bin"), 4);
        store::mark_committed(&p("db/_watermark"), 8);
        // The watermark is metadata → still rewritten every commit (content may
        // change at constant length); the append-only columns are now clean.
        let after = store::dirty_columns();
        assert!(after.iter().all(|dc| dc.path == p("db/_watermark")));
    }

    #[test]
    fn noop_truncate_does_not_fault_in_a_lazy_column() {
        store::clear();
        let reads = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut blobs = HashMap::new();
        // A persisted string column (data + offsets) with one row.
        let mut data = Vec::new();
        let mut offs = Vec::new();
        let s = "hi".as_bytes();
        data.extend_from_slice(s);
        offs.extend_from_slice(&0u64.to_le_bytes());
        offs.extend_from_slice(&(s.len() as u64).to_le_bytes());
        blobs.insert(p("db/f/s_data.bin"), data);
        blobs.insert(p("db/f/s_off.bin"), offs);
        store::set_source(Box::new(MapSource {
            blobs,
            reads: reads.clone(),
        }));

        let mut v = VariableColumn::new(p("db/f/s_data.bin"), p("db/f/s_off.bin")).unwrap();
        assert_eq!(v.len(), 1);
        // Recovery calls truncate_to_rows(anchor) on every column at open; a
        // no-op truncation (rows == len) must NOT fault the column in — the
        // load-bearing fix for partial hydrate.
        v.truncate_to_rows(1).unwrap();
        assert!(reads.borrow().is_empty(), "no-op truncate must not read");
        // A real read still faults it in.
        assert_eq!(v.read_string(0).unwrap(), "hi");
        assert!(!reads.borrow().is_empty());
    }

    #[test]
    fn no_source_behaves_eagerly_like_before() {
        store::clear();
        // With no lazy source registered, ensure() creates a resident empty
        // arena and everything behaves as the pre-#2 eager path.
        let mut c = FixedColumn::new(p("n/u64.bin"), 8).unwrap();
        assert!(c.is_empty());
        c.append_u64(1).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c.read_u64(0).unwrap(), 1);
    }
}
