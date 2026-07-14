//! Browser persistence glue — the async hydrate/commit boundary (wasm32 only).
//!
//! The arena [`crate::store`] is volatile. This module moves its opaque
//! path→bytes blobs to and from a durable browser store at the open/commit
//! boundary, so a follower resumes after a tab reload. Two backends, chosen
//! per-project (the design note keeps both — `docs/proposals/wasm-runtime.md`):
//!
//! - [`Backend::IndexedDb`] — one object store of **keyed blobs**, key = the
//!   column path, value = the column bytes. **Eager**: [`hydrate`] loads every
//!   blob up front; [`commit`] rewrites the whole snapshot. Works on the main
//!   thread and in a Worker; the broadest-support fallback.
//! - [`Backend::Opfs`] — **per-column files** under a `<db>/` directory in the
//!   Origin Private File System, read/written through **`createSyncAccessHandle`**
//!   (Worker-only). **Lazy / partial** (#110 follow-up #2): at open we open a
//!   sync-access handle per existing file (async, metadata only — no bytes read)
//!   and register a synchronous [`store::LazySource`], so a column faults in
//!   **synchronously** on first access and columns never read never load.
//!   [`commit`] writes only each column's grown byte-suffix (append-only tails).
//!   Requires a Worker context (the engine runs in the Worker — the #2 topology);
//!   this supersedes #110 #1's helper-worker OPFS I/O.
//!
//! Both backends are `async` and called ONLY here, at the boundary. The per-row
//! column API stays synchronous (arena slice math / sync-access-handle reads),
//! so the generated read path is never async-colored — the PM constraint. This
//! module is schema-agnostic: it interprets neither the path keys nor the bytes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemReadWriteOptions, FileSystemSyncAccessHandle, IdbDatabase,
    IdbFactory, IdbObjectStore, IdbOpenDbRequest, IdbRequest, IdbTransactionMode, StorageManager,
};

use crate::store;

/// The durable browser store backing the commit boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// IndexedDB keyed-blob object store — eager hydrate, whole-snapshot commit.
    IndexedDb,
    /// OPFS per-column files via sync-access handles — lazy partial hydrate,
    /// incremental tail commit. Worker-only.
    Opfs,
}

impl Backend {
    /// Parse a backend name (`"indexeddb"` / `"idb"` or `"opfs"`), defaulting to
    /// IndexedDB for anything unrecognized (the broadest-support choice).
    pub fn from_str_lossy(s: &str) -> Backend {
        match s.to_ascii_lowercase().as_str() {
            "opfs" => Backend::Opfs,
            _ => Backend::IndexedDb,
        }
    }
}

const OBJECT_STORE: &str = "columns";

/// Hydrate the arena for `db_name` from `backend` (the open boundary).
///
/// - IndexedDB: eager — loads every blob into the arena now.
/// - OPFS: lazy — opens sync-access handles and registers a [`store::LazySource`]
///   so columns fault in on first read. Reads no column bytes here.
pub async fn hydrate(backend: Backend, db_name: &str) -> Result<(), JsValue> {
    match backend {
        Backend::IndexedDb => {
            let entries = idb_load(db_name).await?;
            store::hydrate(entries);
            Ok(())
        }
        Backend::Opfs => opfs_open_lazy(db_name).await,
    }
}

/// Persist the arena for `db_name` to `backend` at commit granularity.
///
/// - IndexedDB: rewrites the whole keyed-blob snapshot.
/// - OPFS: writes only each resident column's grown tail (append-only), or a
///   whole rewrite for a shrink / metadata blob — never a torn tail.
pub async fn commit(backend: Backend, db_name: &str) -> Result<(), JsValue> {
    match backend {
        Backend::IndexedDb => idb_store(db_name, store::dump()).await,
        Backend::Opfs => opfs_commit(db_name).await,
    }
}

// ---- request/transaction awaiting helpers -------------------------------------

/// Await an `IdbRequest`'s `onsuccess`/`onerror`, resolving to its `result`.
fn await_request(req: &IdbRequest) -> JsFuture {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let r = req.clone();
        let onsuccess = Closure::once(Box::new(move || {
            let _ = resolve.call1(&JsValue::NULL, &r.result().unwrap_or(JsValue::NULL));
        }) as Box<dyn FnOnce()>);
        req.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
        onsuccess.forget();

        let onerror = Closure::once(Box::new(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB request failed"));
        }) as Box<dyn FnOnce()>);
        req.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
    });
    JsFuture::from(promise)
}

// ---- IndexedDB (eager) --------------------------------------------------------

/// The IndexedDB factory from either a `Window` or a `WorkerGlobalScope`.
fn idb_factory() -> Result<IdbFactory, JsValue> {
    if let Some(win) = web_sys::window() {
        return win
            .indexed_db()?
            .ok_or_else(|| JsValue::from_str("IndexedDB unavailable in this window"));
    }
    let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
    global
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("IndexedDB unavailable in this worker"))
}

/// Open (creating the object store on first use) the database `db_name`.
async fn idb_open(db_name: &str) -> Result<IdbDatabase, JsValue> {
    let factory = idb_factory()?;
    let open_req: IdbOpenDbRequest = factory.open_with_u32(db_name, 1)?;

    let onupgrade = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            let req: IdbOpenDbRequest = target.unchecked_into();
            if let Ok(result) = req.result() {
                let db: IdbDatabase = result.unchecked_into();
                let _ = db.create_object_store(OBJECT_STORE);
            }
        }
    });
    open_req.set_onupgradeneeded(Some(onupgrade.as_ref().unchecked_ref()));
    onupgrade.forget();

    let db_val = await_request(open_req.as_ref()).await?;
    Ok(db_val.unchecked_into())
}

async fn idb_load(db_name: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, JsValue> {
    let db = idb_open(db_name).await?;
    let tx = db.transaction_with_str_and_mode(OBJECT_STORE, IdbTransactionMode::Readonly)?;
    let store_h: IdbObjectStore = tx.object_store(OBJECT_STORE)?;

    let keys: Array = await_request(&store_h.get_all_keys()?).await?.unchecked_into();
    let values: Array = await_request(&store_h.get_all()?).await?.unchecked_into();

    let mut out = Vec::with_capacity(keys.length() as usize);
    for i in 0..keys.length() {
        let path = keys.get(i).as_string().unwrap_or_default();
        let bytes = Uint8Array::new(&values.get(i)).to_vec();
        out.push((PathBuf::from(path), bytes));
    }
    db.close();
    Ok(out)
}

async fn idb_store(db_name: &str, entries: Vec<(PathBuf, Vec<u8>)>) -> Result<(), JsValue> {
    let db = idb_open(db_name).await?;
    let tx = db.transaction_with_str_and_mode(OBJECT_STORE, IdbTransactionMode::Readwrite)?;
    let store_h: IdbObjectStore = tx.object_store(OBJECT_STORE)?;

    let _ = await_request(&store_h.clear()?).await;
    for (path, bytes) in &entries {
        let key = JsValue::from_str(&path.to_string_lossy());
        let val = Uint8Array::from(bytes.as_slice());
        let _ = await_request(&store_h.put_with_key(&val, &key)?).await?;
    }
    db.close();
    Ok(())
}

// ---- OPFS (lazy partial hydrate via sync-access handles) ----------------------

thread_local! {
    /// One open sync-access handle per persisted column file, keyed by the
    /// full arena path. Opened at [`opfs_open_lazy`] (async) and kept open for
    /// the session so reads/writes are synchronous. A single follower touches
    /// each file, so holding the exclusive handle is fine.
    static OPFS_HANDLES: RefCell<HashMap<PathBuf, FileSystemSyncAccessHandle>> =
        RefCell::new(HashMap::new());
    /// The `<db>/` OPFS directory handle, cached so commit can create files for
    /// columns that appeared this session (no pre-opened handle).
    static OPFS_DIR: RefCell<Option<FileSystemDirectoryHandle>> = const { RefCell::new(None) };
}

/// Escape an arena path into a flat OPFS filename (`/` and `%` are the only
/// reserved chars; both round-trip through [`opfs_decode`]).
fn opfs_encode(path: &str) -> String {
    path.replace('%', "%25").replace('/', "%2F")
}
fn opfs_decode(name: &str) -> String {
    name.replace("%2F", "/").replace("%25", "%")
}

/// The `StorageManager` from a `Window` or a `WorkerGlobalScope`.
fn storage_manager() -> Result<StorageManager, JsValue> {
    if let Some(win) = web_sys::window() {
        return Ok(win.navigator().storage());
    }
    let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
    Ok(global.navigator().storage())
}

/// Open (creating if absent) the `<db>/` directory under the OPFS root.
async fn opfs_directory(db_name: &str) -> Result<FileSystemDirectoryHandle, JsValue> {
    let root_val = JsFuture::from(storage_manager()?.get_directory()).await?;
    let root: FileSystemDirectoryHandle = root_val.dyn_into()?;
    let opts = FileSystemGetDirectoryOptions::new();
    opts.set_create(true);
    let dir_val = JsFuture::from(root.get_directory_handle_with_options(db_name, &opts)).await?;
    dir_val.dyn_into()
}

/// The lazy fault-in source over the OPFS sync-access handle map. `len` answers
/// from `getSize()` (no bytes read); `read` faults the whole column in
/// synchronously. Both are opaque — keyed by the arena path.
struct OpfsSource;

impl store::LazySource for OpfsSource {
    fn len(&self, path: &Path) -> Option<usize> {
        OPFS_HANDLES.with(|h| {
            h.borrow()
                .get(path)
                .and_then(|ah| ah.get_size().ok())
                .map(|sz| sz as usize)
        })
    }

    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        OPFS_HANDLES.with(|h| {
            let map = h.borrow();
            let ah = map.get(path)?;
            let size = ah.get_size().ok()? as usize;
            let mut buf = vec![0u8; size];
            let opts = FileSystemReadWriteOptions::new();
            opts.set_at(0.0);
            match ah.read_with_u8_array_and_options(&mut buf, &opts) {
                Ok(n) => {
                    buf.truncate(n as usize);
                    // Debug-level diagnostic (suppressed by default) so the
                    // partial-hydrate property is observable: exactly which
                    // columns faulted in, and that untouched ones never do.
                    web_sys::console::debug_1(&JsValue::from_str(&format!(
                        "[forgedb] fault-in {}",
                        path.display()
                    )));
                    Some(buf)
                }
                Err(e) => {
                    web_sys::console::error_1(&e);
                    None
                }
            }
        })
    }
}

/// Open a sync-access handle per existing column file and register the lazy
/// source. Reads NO column bytes — only metadata (which files exist, their
/// sizes are read on demand). This is the partial-hydrate open boundary.
async fn opfs_open_lazy(db_name: &str) -> Result<(), JsValue> {
    let dir = opfs_directory(db_name).await?;
    OPFS_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
    OPFS_HANDLES.with(|h| h.borrow_mut().clear());

    // Drive the async directory iterator: each entry is `[name, handle]`.
    let iter = dir.entries();
    loop {
        let next = JsFuture::from(iter.next()?).await?;
        if Reflect::get(&next, &"done".into())?.as_bool().unwrap_or(true) {
            break;
        }
        let pair: Array = Reflect::get(&next, &"value".into())?.unchecked_into();
        let name = pair.get(0).as_string().unwrap_or_default();
        let file_handle: FileSystemFileHandle = pair.get(1).dyn_into()?;
        let ah_val = JsFuture::from(file_handle.create_sync_access_handle()).await?;
        let ah: FileSystemSyncAccessHandle = ah_val.dyn_into()?;
        let arena_path = PathBuf::from(format!("{db_name}/{}", opfs_decode(&name)));
        OPFS_HANDLES.with(|h| {
            h.borrow_mut().insert(arena_path, ah);
        });
    }

    store::set_source(Box::new(OpfsSource));
    Ok(())
}

/// Get (or async-create) the sync-access handle for `arena_path`, for a column
/// that appeared this session and has no pre-opened handle. The flat filename is
/// the arena path escaped and stored under `<db>/`.
async fn opfs_handle_for(db_name: &str, arena_path: &Path) -> Result<FileSystemSyncAccessHandle, JsValue> {
    if let Some(ah) = OPFS_HANDLES.with(|h| h.borrow().get(arena_path).cloned()) {
        return Ok(ah);
    }
    let dir = OPFS_DIR
        .with(|d| d.borrow().clone())
        .ok_or_else(|| JsValue::from_str("OPFS directory not opened"))?;
    // The persisted filename is the arena path relative to the `<db>/` dir,
    // escaped. Strip the `<db>/` prefix so a re-open decodes to the same path.
    let rel = arena_path
        .to_string_lossy()
        .strip_prefix(&format!("{db_name}/"))
        .unwrap_or(&arena_path.to_string_lossy())
        .to_string();
    let opts = FileSystemGetFileOptions::new();
    opts.set_create(true);
    let fh_val = JsFuture::from(dir.get_file_handle_with_options(&opfs_encode(&rel), &opts)).await?;
    let fh: FileSystemFileHandle = fh_val.dyn_into()?;
    let ah_val = JsFuture::from(fh.create_sync_access_handle()).await?;
    let ah: FileSystemSyncAccessHandle = ah_val.dyn_into()?;
    OPFS_HANDLES.with(|h| {
        h.borrow_mut().insert(arena_path.to_path_buf(), ah.clone());
    });
    Ok(ah)
}

/// Incremental commit: write only each resident column's grown tail (or a whole
/// rewrite for a shrink / metadata blob) via its sync-access handle, then flush.
/// Never publishes a torn tail — a crash mid-commit leaves a recoverable prefix,
/// and the un-advanced columns re-emit their tails next commit.
async fn opfs_commit(db_name: &str) -> Result<(), JsValue> {
    // Ensure the directory is available even if commit runs before a lazy open
    // registered one (e.g. a fresh DB whose first hydrate found no files).
    if OPFS_DIR.with(|d| d.borrow().is_none()) {
        let dir = opfs_directory(db_name).await?;
        OPFS_DIR.with(|d| *d.borrow_mut() = Some(dir));
    }

    // Ordering is load-bearing: append-only column tails (`truncate == false`)
    // are written + flushed FIRST, then whole-rewrite entries (`truncate == true`,
    // which includes the `_watermark` commit marker) LAST. So the persisted
    // watermark never runs ahead of the durable columns — a crash between the two
    // leaves the columns ahead of the watermark, and re-streaming those frames on
    // resume is logically idempotent (the generated `all()`/`get()` resolve one
    // record per id, so a re-applied insert only appends a dead superseded row).
    let mut dirty = store::dirty_columns();
    dirty.sort_by_key(|dc| dc.truncate);
    for dc in dirty {
        let ah = opfs_handle_for(db_name, &dc.path).await?;
        if dc.truncate {
            ah.truncate_with_f64(0.0)?;
        }
        let opts = FileSystemReadWriteOptions::new();
        opts.set_at(dc.offset as f64);
        // `write_with_u8_array_and_options` needs `&[u8]`; move into a local so
        // the arena borrow is not held across the FFI call.
        let bytes = dc.bytes;
        ah.write_with_u8_array_and_options(&bytes, &opts)?;
        ah.flush()?;
        store::mark_committed(&dc.path, dc.offset + bytes.len());
    }
    Ok(())
}
