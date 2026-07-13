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
//! ## The async boundary
//!
//! Positional reads/writes over these arenas are **synchronous** (plain slice
//! math), exactly like the file engine — that is what keeps the generated
//! per-row API unchanged. Async is quarantined to two module functions the
//! persistence glue calls at the open/commit boundary:
//!
//! - [`hydrate`] — load column blobs from IndexedDB/OPFS into the store on open.
//! - [`dump`] — snapshot every arena as `(path, bytes)` to write back on commit.
//!
//! Both move **opaque path→bytes blobs**; this module knows no model, field, or
//! relation — it is schema-agnostic substrate.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

thread_local! {
    static STORE: RefCell<HashMap<PathBuf, Vec<u8>>> = RefCell::new(HashMap::new());
}

/// Ensure an arena exists for `path` (empty if new). Mirrors the file engine
/// creating/opening the column file in each column's `new`.
pub(crate) fn ensure(path: &Path) {
    STORE.with(|s| {
        s.borrow_mut().entry(path.to_path_buf()).or_default();
    });
}

/// Read the arena bytes for `path` (empty slice if absent) through `f`.
pub(crate) fn with_bytes<R>(path: &Path, f: impl FnOnce(&[u8]) -> R) -> R {
    STORE.with(|s| {
        let map = s.borrow();
        let empty: &[u8] = &[];
        f(map.get(path).map(Vec::as_slice).unwrap_or(empty))
    })
}

/// Mutate the arena bytes for `path` (creating it if absent) through `f`.
pub(crate) fn with_bytes_mut<R>(path: &Path, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    STORE.with(|s| {
        let mut map = s.borrow_mut();
        f(map.entry(path.to_path_buf()).or_default())
    })
}

/// Current byte length of the arena for `path` (`0` if absent).
pub(crate) fn byte_len(path: &Path) -> usize {
    STORE.with(|s| s.borrow().get(path).map(Vec::len).unwrap_or(0))
}

/// Load column blobs into the store on open (the hydrate boundary).
///
/// Called by the persistence glue after reading blobs from IndexedDB/OPFS, before
/// the generated `Database::open_at` constructs its columns. Each `(path, bytes)`
/// is inserted verbatim; the column `new` calls then read already-populated
/// arenas. Opaque: neither key nor value is interpreted here.
pub fn hydrate(entries: impl IntoIterator<Item = (PathBuf, Vec<u8>)>) {
    STORE.with(|s| {
        let mut map = s.borrow_mut();
        for (k, v) in entries {
            map.insert(k, v);
        }
    });
}

/// Snapshot every arena as `(path, bytes)` for the commit boundary.
///
/// The persistence glue calls this and writes the blobs to IndexedDB/OPFS in one
/// transaction. Clones the bytes so the caller can hand them to an async task
/// without holding the `RefCell` borrow. Field-blind: opaque keys + opaque bytes.
pub fn dump() -> Vec<(PathBuf, Vec<u8>)> {
    STORE.with(|s| {
        s.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    })
}

/// Clear the whole backing store. Used by a fresh open and by tests.
pub fn clear() {
    STORE.with(|s| s.borrow_mut().clear());
}

/// Put an opaque blob under `path` (overwriting). The transport uses this to
/// stash follower metadata — e.g. the resume watermark — as just another arena
/// entry, so it is carried to durable storage by the same [`dump`]/[`hydrate`]
/// path as the columns. Opaque: this module never interprets the bytes.
pub fn put(path: impl Into<PathBuf>, bytes: Vec<u8>) {
    STORE.with(|s| {
        s.borrow_mut().insert(path.into(), bytes);
    });
}

/// Read the opaque blob at `path` (`None` if absent). Companion to [`put`].
pub fn get(path: &Path) -> Option<Vec<u8>> {
    STORE.with(|s| s.borrow().get(path).cloned())
}
