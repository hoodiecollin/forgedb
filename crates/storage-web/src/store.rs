//! The in-memory backing store — the arena behind every column on this target.
//!
//! Each column file the native engine would put on disk is here a keyed byte
//! arena: the **path string is the key** (the "signature-preservation trick"
//! from `docs/proposals/wasm-runtime.md`). The generated code still calls
//! `FixedColumn::new(PathBuf::from(col_path), size)`; on this target the path is
//! just the arena key instead of a filesystem path.
//!
//! The store is a `thread_local` map because `wasm32-unknown-unknown` is
//! single-threaded — there is exactly one agent (the follower) touching it, so
//! no lock is needed and `RefCell` borrows never contend.
//!
//! ## The async boundary and lazy fault-in
//!
//! Positional reads/writes over these arenas are **synchronous** (plain slice
//! math), exactly like the file engine — that is what keeps the generated
//! per-row API unchanged. Async is quarantined to the persistence glue's
//! open/commit boundary. Two hydrate strategies both preserve the sync per-row
//! API (#110 follow-up #2):
//!
//! - **Eager** — [`hydrate`] loads every column blob up front (the IndexedDB
//!   path; also any target without synchronous fault-in).
//! - **Lazy / partial** — [`set_source`] registers a [`LazySource`] (an OPFS
//!   sync-access handle map, Worker-only). A column's length is answered from
//!   the source's `len` **without** reading its bytes; the bytes are faulted in
//!   **synchronously** (whole-column) on the first real read. Columns never read
//!   never load. The fault-in is sync (sync-access-handle `read`), so the
//!   generated per-row API is *not* async-colored — the PM constraint.
//!
//! Both strategies move **opaque path→bytes blobs**; this module knows no model,
//! field, or relation — it is schema-agnostic substrate.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A durable, synchronously-readable backing for lazily-hydrated columns.
///
/// Implemented by the wasm persistence glue over a map of OPFS sync-access
/// handles. `len` must answer the on-disk byte length *without* reading bytes
/// (an OPFS `getSize()`); `read` faults the whole column in. Both are opaque —
/// keyed by the arena path, returning raw bytes. `None` from either means "no
/// such column is source-backed" (a genuinely new column created this session).
pub trait LazySource {
    /// On-disk byte length of the column at `path`, or `None` if not backed.
    fn len(&self, path: &Path) -> Option<usize>;
    /// Full bytes of the column at `path`, or `None` if not backed / on error.
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
}

/// One column's pending durable write, computed by [`dirty_columns`] for the
/// incremental (OPFS) commit path. `truncate` rewrites the whole file from 0
/// (a shrink, or a `put` metadata blob); otherwise `bytes` is the append-only
/// suffix to write at `offset`.
pub struct DirtyColumn {
    pub path: PathBuf,
    pub offset: usize,
    pub bytes: Vec<u8>,
    pub truncate: bool,
}

thread_local! {
    /// Resident column bytes (loaded/created this session).
    static STORE: RefCell<HashMap<PathBuf, Vec<u8>>> = RefCell::new(HashMap::new());
    /// Paths whose full content is resident in `STORE` (loaded from a source,
    /// created fresh, or appended to). A source-backed path NOT in this set is
    /// lazy: its length comes from the source, its bytes are not yet read.
    static LOADED: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
    /// The lazy fault-in source, when hydrating partially (OPFS). `None` in the
    /// eager path (IndexedDB) and natively — then every arena is resident.
    static SOURCE: RefCell<Option<Box<dyn LazySource>>> = const { RefCell::new(None) };
    /// Per-path bytes already durable, for the incremental tail commit.
    static COMMITTED_LEN: RefCell<HashMap<PathBuf, usize>> = RefCell::new(HashMap::new());
    /// Paths written via [`put`] (opaque metadata blobs, e.g. the watermark).
    /// These can change content without changing length, so the incremental
    /// commit always rewrites them whole rather than tail-diffing.
    static META: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
}

/// Register a lazy fault-in source (the OPFS handle map). Enables partial
/// hydrate: columns load on first read, not at open. Call after [`clear`] and
/// before the generated `Database::open_at` constructs its columns.
pub fn set_source(source: Box<dyn LazySource>) {
    SOURCE.with(|s| *s.borrow_mut() = Some(source));
}

/// Fault a source-backed column into `STORE` if it is not already resident.
/// Synchronous — the whole point (no async coloring of the per-row API). A
/// no-op when there is no source, when the path is already resident, or when
/// the source does not back this path (a fresh column).
fn ensure_loaded(path: &Path) {
    if LOADED.with(|l| l.borrow().contains(path)) {
        return;
    }
    SOURCE.with(|s| {
        if let Some(src) = s.borrow().as_ref() {
            if let Some(bytes) = src.read(path) {
                let len = bytes.len();
                STORE.with(|st| {
                    st.borrow_mut().insert(path.to_path_buf(), bytes);
                });
                // Loaded content is already durable — seed the commit watermark
                // so an unmodified column is not re-written on the next commit.
                COMMITTED_LEN.with(|c| {
                    c.borrow_mut().insert(path.to_path_buf(), len);
                });
                LOADED.with(|l| {
                    l.borrow_mut().insert(path.to_path_buf());
                });
            }
        }
    });
}

/// Ensure an arena exists for `path`. Mirrors the file engine creating/opening
/// the column file in each column's `new`. When a lazy source backs this path
/// it is left **lazy** (not loaded, not zeroed) so partial hydrate holds;
/// otherwise a fresh empty resident arena is created.
pub(crate) fn ensure(path: &Path) {
    if LOADED.with(|l| l.borrow().contains(path)) {
        return;
    }
    // Source-backed and unread → stay lazy; its length answers from the source.
    let backed = SOURCE.with(|s| s.borrow().as_ref().and_then(|src| src.len(path)).is_some());
    if backed {
        return;
    }
    // A genuinely new column: resident and empty.
    STORE.with(|s| {
        s.borrow_mut().entry(path.to_path_buf()).or_default();
    });
    LOADED.with(|l| {
        l.borrow_mut().insert(path.to_path_buf());
    });
}

/// Read the arena bytes for `path` (empty slice if absent) through `f`. Faults
/// the column in synchronously first if it is lazily source-backed.
pub(crate) fn with_bytes<R>(path: &Path, f: impl FnOnce(&[u8]) -> R) -> R {
    ensure_loaded(path);
    STORE.with(|s| {
        let map = s.borrow();
        let empty: &[u8] = &[];
        f(map.get(path).map(Vec::as_slice).unwrap_or(empty))
    })
}

/// Mutate the arena bytes for `path` (creating it if absent) through `f`. Faults
/// the column in first, so an append lands on the full durable prefix — never on
/// a phantom-empty arena (the row-alignment hazard the PM flagged).
pub(crate) fn with_bytes_mut<R>(path: &Path, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    ensure_loaded(path);
    LOADED.with(|l| {
        l.borrow_mut().insert(path.to_path_buf());
    });
    STORE.with(|s| {
        let mut map = s.borrow_mut();
        f(map.entry(path.to_path_buf()).or_default())
    })
}

/// Current byte length of the arena for `path`. For a lazy source-backed column
/// this is the source's on-disk length — answered **without** reading the bytes,
/// so `len()`-driven recovery/row-count math does not force a full hydrate.
pub(crate) fn byte_len(path: &Path) -> usize {
    if LOADED.with(|l| l.borrow().contains(path)) {
        return STORE.with(|s| s.borrow().get(path).map(Vec::len).unwrap_or(0));
    }
    if let Some(n) = SOURCE.with(|s| s.borrow().as_ref().and_then(|src| src.len(path))) {
        return n;
    }
    STORE.with(|s| s.borrow().get(path).map(Vec::len).unwrap_or(0))
}

/// Load column blobs into the store on open (the eager hydrate boundary).
///
/// Called by the persistence glue after reading blobs from IndexedDB, before the
/// generated `Database::open_at` constructs its columns. Each `(path, bytes)` is
/// inserted verbatim and marked resident + durable. Opaque: neither key nor value
/// is interpreted here.
pub fn hydrate(entries: impl IntoIterator<Item = (PathBuf, Vec<u8>)>) {
    STORE.with(|s| {
        let mut store = s.borrow_mut();
        LOADED.with(|l| {
            let mut loaded = l.borrow_mut();
            COMMITTED_LEN.with(|c| {
                let mut committed = c.borrow_mut();
                for (k, v) in entries {
                    committed.insert(k.clone(), v.len());
                    loaded.insert(k.clone());
                    store.insert(k, v);
                }
            });
        });
    });
}

/// Snapshot every arena as `(path, bytes)` for the eager (whole-DB) commit
/// boundary — the IndexedDB path. Clones the bytes so the caller can hand them
/// to an async task without holding the `RefCell` borrow. Field-blind.
///
/// (The OPFS path uses [`dirty_columns`] instead, writing only grown tails.)
pub fn dump() -> Vec<(PathBuf, Vec<u8>)> {
    STORE.with(|s| {
        s.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    })
}

/// Compute the pending durable writes for the incremental (OPFS) commit: for
/// each **resident** arena, the append-only suffix past its committed length (or
/// a whole rewrite for a shrink or a `put` metadata blob). Lazily-loaded columns
/// that were never read are already durable and byte-for-byte unchanged, so they
/// are absent here. Does not mutate the commit watermark — call [`mark_committed`]
/// after each successful write so a failed commit is retried, not lost.
pub fn dirty_columns() -> Vec<DirtyColumn> {
    STORE.with(|s| {
        let store = s.borrow();
        COMMITTED_LEN.with(|c| {
            let committed = c.borrow();
            META.with(|m| {
                let meta = m.borrow();
                let mut out = Vec::new();
                for (path, buf) in store.iter() {
                    let is_meta = meta.contains(path);
                    let done = committed.get(path).copied().unwrap_or(0);
                    if is_meta || buf.len() < done {
                        // Metadata blob (may change content at same length) or a
                        // shrink → rewrite the whole file from offset 0.
                        out.push(DirtyColumn {
                            path: path.clone(),
                            offset: 0,
                            bytes: buf.clone(),
                            truncate: true,
                        });
                    } else if buf.len() > done {
                        out.push(DirtyColumn {
                            path: path.clone(),
                            offset: done,
                            bytes: buf[done..].to_vec(),
                            truncate: false,
                        });
                    }
                }
                out
            })
        })
    })
}

/// Record that `len` bytes of `path` are now durable (the incremental commit
/// calls this after each successful write).
pub fn mark_committed(path: &Path, len: usize) {
    COMMITTED_LEN.with(|c| {
        c.borrow_mut().insert(path.to_path_buf(), len);
    });
}

/// Clear the whole backing store — resident bytes, the lazy source, and all
/// commit bookkeeping. Used by a fresh open (before a source is registered) and
/// by tests.
pub fn clear() {
    STORE.with(|s| s.borrow_mut().clear());
    LOADED.with(|l| l.borrow_mut().clear());
    SOURCE.with(|s| *s.borrow_mut() = None);
    COMMITTED_LEN.with(|c| c.borrow_mut().clear());
    META.with(|m| m.borrow_mut().clear());
}

/// Put an opaque blob under `path` (overwriting). The transport uses this to
/// stash follower metadata — e.g. the resume watermark — as just another arena
/// entry, carried to durable storage by the same commit path as the columns.
/// Marked as metadata so the incremental commit rewrites it whole (its content
/// can change without changing length). Opaque: never interprets the bytes.
pub fn put(path: impl Into<PathBuf>, bytes: Vec<u8>) {
    let path = path.into();
    STORE.with(|s| {
        s.borrow_mut().insert(path.clone(), bytes);
    });
    LOADED.with(|l| {
        l.borrow_mut().insert(path.clone());
    });
    META.with(|m| {
        m.borrow_mut().insert(path);
    });
}

/// Read the opaque blob at `path` (`None` if absent). Companion to [`put`];
/// faults a lazily source-backed blob in first.
pub fn get(path: &Path) -> Option<Vec<u8>> {
    ensure_loaded(path);
    STORE.with(|s| s.borrow().get(path).cloned())
}
