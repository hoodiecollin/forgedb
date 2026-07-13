//! Browser persistence glue — the async hydrate/commit boundary (wasm32 only).
//!
//! The arena [`crate::store`] is volatile. This module moves its opaque
//! path→bytes blobs to and from a durable browser store at the open/commit
//! boundary, so a follower resumes after a tab reload. Two backends, chosen
//! per-project (the design note keeps both — `docs/proposals/wasm-runtime.md`):
//!
//! - [`Backend::IndexedDb`] — one object store of **keyed blobs**, key =
//!   the column path, value = the column bytes (the design-note milestone-1
//!   layout). Broadest support; works on the main thread.
//! - [`Backend::Opfs`] — a single whole-DB **snapshot file** per database under
//!   the Origin Private File System. Commit-granularity, whole-DB — exactly the
//!   accepted milestone-1 durability model. (Per-column OPFS files / sync-access
//!   handles in a Worker are a later optimization; a snapshot file needs no
//!   directory iteration and no Worker.)
//!
//! Both are `async` and called ONLY here, at the boundary. The per-row column
//! API stays synchronous (arena slice math). This module is schema-agnostic: it
//! interprets neither the path keys nor the blob bytes.

use std::path::PathBuf;

use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    IdbDatabase, IdbFactory, IdbObjectStore, IdbOpenDbRequest, IdbRequest, IdbTransactionMode,
};

/// The durable browser store backing the commit boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// IndexedDB keyed-blob object store (design-note milestone-1 layout).
    IndexedDb,
    /// OPFS single whole-DB snapshot file.
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

/// Load all persisted column blobs for `db_name` from `backend`.
pub async fn load(backend: Backend, db_name: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, JsValue> {
    match backend {
        Backend::IndexedDb => idb_load(db_name).await,
        Backend::Opfs => opfs_load(db_name).await,
    }
}

/// Persist `entries` (opaque path→bytes) for `db_name` to `backend`, atomically
/// at commit granularity.
pub async fn store(
    backend: Backend,
    db_name: &str,
    entries: Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), JsValue> {
    match backend {
        Backend::IndexedDb => idb_store(db_name, entries).await,
        Backend::Opfs => opfs_store(db_name, entries).await,
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

// ---- IndexedDB ----------------------------------------------------------------

/// The IndexedDB factory from either a `Window` or a `WorkerGlobalScope`.
fn idb_factory() -> Result<IdbFactory, JsValue> {
    if let Some(win) = web_sys::window() {
        return win
            .indexed_db()?
            .ok_or_else(|| JsValue::from_str("IndexedDB unavailable in this window"));
    }
    // Worker context: reach IndexedDB via the global scope.
    let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
    global
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("IndexedDB unavailable in this worker"))
}

/// Open (creating the object store on first use) the database `db_name`.
async fn idb_open(db_name: &str) -> Result<IdbDatabase, JsValue> {
    let factory = idb_factory()?;
    let open_req: IdbOpenDbRequest = factory.open_with_u32(db_name, 1)?;

    // First-open / version bump: create the single keyed-blob object store.
    // `onupgradeneeded` fires only when the DB is being created or upgraded, so
    // creating unconditionally here is safe (it never runs against an existing
    // store of the same version).
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
    let store: IdbObjectStore = tx.object_store(OBJECT_STORE)?;

    let keys: Array = await_request(&store.get_all_keys()?).await?.unchecked_into();
    let values: Array = await_request(&store.get_all()?).await?.unchecked_into();

    let mut out = Vec::with_capacity(keys.length() as usize);
    for i in 0..keys.length() {
        let key = keys.get(i);
        let path = key.as_string().unwrap_or_default();
        let val = values.get(i);
        let bytes = Uint8Array::new(&val).to_vec();
        out.push((PathBuf::from(path), bytes));
    }
    db.close();
    Ok(out)
}

async fn idb_store(db_name: &str, entries: Vec<(PathBuf, Vec<u8>)>) -> Result<(), JsValue> {
    let db = idb_open(db_name).await?;
    let tx = db.transaction_with_str_and_mode(OBJECT_STORE, IdbTransactionMode::Readwrite)?;
    let store: IdbObjectStore = tx.object_store(OBJECT_STORE)?;

    // Fresh whole-DB snapshot: clear then re-put every arena.
    let _ = await_request(&store.clear()?).await;
    for (path, bytes) in &entries {
        let key = JsValue::from_str(&path.to_string_lossy());
        let val = Uint8Array::from(bytes.as_slice());
        let _ = await_request(&store.put_with_key(&val, &key)?).await?;
    }
    db.close();
    Ok(())
}

// ---- OPFS (single snapshot file) ----------------------------------------------

/// Encode all entries into one length-prefixed snapshot blob:
/// `[4:count]( [2:path_len][path][4:bytes_len][bytes] )*`.
fn encode_entries(entries: &[(PathBuf, Vec<u8>)]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (path, bytes) in entries {
        let p = path.to_string_lossy();
        let pb = p.as_bytes();
        buf.extend_from_slice(&(pb.len() as u16).to_le_bytes());
        buf.extend_from_slice(pb);
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }
    buf
}

/// Decode a snapshot blob produced by [`encode_entries`]. Torn/short input yields
/// whatever prefix parsed cleanly (a fresh DB simply has no snapshot file).
fn decode_entries(buf: &[u8]) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    if buf.len() < 4 {
        return out;
    }
    let count = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    let mut off = 4;
    for _ in 0..count {
        if off + 2 > buf.len() {
            break;
        }
        let plen = u16::from_le_bytes(buf[off..off + 2].try_into().unwrap()) as usize;
        off += 2;
        if off + plen > buf.len() {
            break;
        }
        let path = String::from_utf8_lossy(&buf[off..off + plen]).into_owned();
        off += plen;
        if off + 4 > buf.len() {
            break;
        }
        let blen = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if off + blen > buf.len() {
            break;
        }
        out.push((PathBuf::from(path), buf[off..off + blen].to_vec()));
        off += blen;
    }
    out
}

/// The OPFS root directory handle (`navigator.storage.getDirectory()`).
async fn opfs_root() -> Result<web_sys::FileSystemDirectoryHandle, JsValue> {
    // Main-thread path (the follower runs on the page). A Worker `WorkerNavigator`
    // path can be added when OPFS sync-access handles land; async OPFS file I/O
    // works fine on the main thread for the whole-DB snapshot.
    let win = web_sys::window()
        .ok_or_else(|| JsValue::from_str("OPFS requires a Window (main-thread) context"))?;
    let storage = win.navigator().storage();
    let dir = JsFuture::from(storage.get_directory()).await?;
    Ok(dir.unchecked_into())
}

fn snapshot_file_name(db_name: &str) -> String {
    format!("{}.forgedb", db_name.replace('/', "_"))
}

async fn opfs_load(db_name: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, JsValue> {
    let root = opfs_root().await?;
    let name = snapshot_file_name(db_name);
    // Missing file (fresh DB) → empty. `getFileHandle` without `create` rejects.
    let handle = match JsFuture::from(root.get_file_handle(&name)).await {
        Ok(h) => h,
        Err(_) => return Ok(Vec::new()),
    };
    let file_handle: web_sys::FileSystemFileHandle = handle.unchecked_into();
    let file_val = JsFuture::from(file_handle.get_file()).await?;
    let file: web_sys::File = file_val.unchecked_into();
    let buf = JsFuture::from(file.array_buffer()).await?;
    let bytes = Uint8Array::new(&buf).to_vec();
    Ok(decode_entries(&bytes))
}

async fn opfs_store(db_name: &str, entries: Vec<(PathBuf, Vec<u8>)>) -> Result<(), JsValue> {
    let root = opfs_root().await?;
    let name = snapshot_file_name(db_name);
    let opts = web_sys::FileSystemGetFileOptions::new();
    opts.set_create(true);
    let handle = JsFuture::from(root.get_file_handle_with_options(&name, &opts)).await?;
    let file_handle: web_sys::FileSystemFileHandle = handle.unchecked_into();

    let writable_val = JsFuture::from(file_handle.create_writable()).await?;
    let writable: web_sys::FileSystemWritableFileStream = writable_val.unchecked_into();

    let blob = encode_entries(&entries);
    let arr = Uint8Array::from(blob.as_slice());
    // write(BufferSource) returns a Promise.
    JsFuture::from(writable.write_with_buffer_source(&arr.buffer())?).await?;
    JsFuture::from(writable.close()).await?;
    Ok(())
}
