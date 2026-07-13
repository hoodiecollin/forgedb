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
//! - [`Backend::Opfs`] — **per-column files** under a `<db>/` directory in the
//!   Origin Private File System, written through **`createSyncAccessHandle`** in
//!   a dedicated **Web Worker** (the only context where the synchronous OPFS
//!   handle API exists). `storage-web` spawns that Worker itself from an embedded
//!   Blob URL, so the crate stays self-contained — no external asset, no codegen
//!   change. Each arena column `[path, bytes]` maps to one file `<db>/<flat-path>`
//!   (`/` escaped so the flat name round-trips); the Worker never interprets a
//!   path or a byte.
//!
//! Both backends are `async` and called ONLY here, at the boundary. The per-row
//! column API stays synchronous (arena slice math). This module is
//! schema-agnostic: it interprets neither the path keys nor the blob bytes.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    IdbDatabase, IdbFactory, IdbObjectStore, IdbOpenDbRequest, IdbRequest, IdbTransactionMode,
    MessageEvent, Worker,
};

/// The durable browser store backing the commit boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// IndexedDB keyed-blob object store (design-note milestone-1 layout).
    IndexedDb,
    /// OPFS per-column files written via sync-access handles in a Worker.
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

// ---- OPFS (per-column files via a sync-access-handle Worker) ------------------

/// Source of the dedicated OPFS I/O Worker. It runs in a Worker because
/// `createSyncAccessHandle` — the synchronous OPFS write API — exists ONLY there.
/// The protocol is one request → one reply, keyed by an opaque `id`:
///   `{id, op:'load', db}`  → `{id, ok, entries:[[path, Uint8Array], …]}`
///   `{id, op:'store', db, entries:[[path, Uint8Array], …]}` → `{id, ok}`
/// It writes each entry to its own file `<db>/<flat(path)>` (with `/` escaped so
/// the flat filename round-trips) and prunes files no longer present — a fresh
/// whole-arena snapshot at commit granularity. It interprets no path and no byte.
const OPFS_WORKER_JS: &str = r#"
self.onmessage = async (e) => {
  const { id, op, db, entries } = e.data;
  const enc = (p) => p.replace(/%/g, '%25').replace(/\//g, '%2F');
  const dec = (n) => n.replace(/%2F/g, '/').replace(/%25/g, '%');
  try {
    const root = await navigator.storage.getDirectory();
    const dir = await root.getDirectoryHandle(db, { create: true });
    if (op === 'load') {
      const out = [];
      for await (const [name, handle] of dir.entries()) {
        if (handle.kind !== 'file') continue;
        const ah = await handle.createSyncAccessHandle();
        const size = ah.getSize();
        const buf = new Uint8Array(size);
        ah.read(buf, { at: 0 });
        ah.close();
        out.push([dec(name), buf]);
      }
      self.postMessage({ id, ok: true, entries: out });
    } else if (op === 'store') {
      const keep = new Set(entries.map(([p]) => enc(p)));
      for await (const [name, handle] of dir.entries()) {
        if (!keep.has(name)) { try { await dir.removeEntry(name); } catch (_) {} }
      }
      for (const [path, bytes] of entries) {
        const fh = await dir.getFileHandle(enc(path), { create: true });
        const ah = await fh.createSyncAccessHandle();
        ah.truncate(0);
        ah.write(bytes, { at: 0 });
        ah.flush();
        ah.close();
      }
      self.postMessage({ id, ok: true });
    } else {
      self.postMessage({ id, ok: false, error: 'unknown op ' + op });
    }
  } catch (err) {
    self.postMessage({ id, ok: false, error: String((err && err.stack) || err) });
  }
};
"#;

thread_local! {
    /// One cached OPFS Worker per follower (spawned lazily from the embedded
    /// Blob URL). A read replica commits serially, so a single Worker suffices.
    static OPFS_WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
    /// Monotonic request id (opaque correlation tag for the one-in-flight call).
    static REQ_ID: Cell<f64> = const { Cell::new(0.0) };
}

/// The cached OPFS Worker, spawned on first use from a Blob URL over the embedded
/// [`OPFS_WORKER_JS`] source (so the crate ships no external `.js` asset).
fn opfs_worker() -> Result<Worker, JsValue> {
    OPFS_WORKER.with(|slot| {
        if let Some(w) = slot.borrow().as_ref() {
            return Ok(w.clone());
        }
        let parts = Array::of1(&JsValue::from_str(OPFS_WORKER_JS));
        let bag = web_sys::BlobPropertyBag::new();
        bag.set_type("text/javascript");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &bag)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)?;
        let worker = Worker::new(&url)?;
        *slot.borrow_mut() = Some(worker.clone());
        Ok(worker)
    })
}

fn next_req_id() -> f64 {
    REQ_ID.with(|c| {
        let v = c.get() + 1.0;
        c.set(v);
        v
    })
}

/// Post one request to the OPFS Worker and await its single reply. Errors surface
/// the Worker's `error` string. `entries` is `Some` only for the `store` op.
async fn opfs_call(
    op: &str,
    db_name: &str,
    entries: Option<&[(PathBuf, Vec<u8>)]>,
) -> Result<JsValue, JsValue> {
    let worker = opfs_worker()?;

    let msg = Object::new();
    Reflect::set(&msg, &"id".into(), &JsValue::from_f64(next_req_id()))?;
    Reflect::set(&msg, &"op".into(), &JsValue::from_str(op))?;
    Reflect::set(&msg, &"db".into(), &JsValue::from_str(db_name))?;
    if let Some(entries) = entries {
        let arr = Array::new();
        for (path, bytes) in entries {
            let pair = Array::new();
            pair.push(&JsValue::from_str(&path.to_string_lossy()));
            pair.push(&Uint8Array::from(bytes.as_slice()));
            arr.push(&pair);
        }
        Reflect::set(&msg, &"entries".into(), &arr)?;
    }

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let onmsg = Closure::once(Box::new(move |e: MessageEvent| {
            let _ = resolve.call1(&JsValue::NULL, &e.data());
        }) as Box<dyn FnOnce(MessageEvent)>);
        worker.set_onmessage(Some(onmsg.as_ref().unchecked_ref()));
        onmsg.forget();

        let onerr = Closure::once(Box::new(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("OPFS worker error"));
        }) as Box<dyn FnOnce()>);
        worker.set_onerror(Some(onerr.as_ref().unchecked_ref()));
        onerr.forget();

        let _ = worker.post_message(&msg);
    });

    let data = JsFuture::from(promise).await?;
    let ok = Reflect::get(&data, &"ok".into())?.as_bool().unwrap_or(false);
    if !ok {
        let err = Reflect::get(&data, &"error".into())?
            .as_string()
            .unwrap_or_else(|| "unknown OPFS worker failure".to_string());
        return Err(JsValue::from_str(&format!("OPFS worker: {err}")));
    }
    Ok(data)
}

async fn opfs_load(db_name: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, JsValue> {
    let data = opfs_call("load", db_name, None).await?;
    let entries_val = Reflect::get(&data, &"entries".into())?;
    let arr: Array = entries_val.unchecked_into();
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let pair: Array = arr.get(i).unchecked_into();
        let path = pair.get(0).as_string().unwrap_or_default();
        let bytes = Uint8Array::new(&pair.get(1)).to_vec();
        out.push((PathBuf::from(path), bytes));
    }
    Ok(out)
}

async fn opfs_store(db_name: &str, entries: Vec<(PathBuf, Vec<u8>)>) -> Result<(), JsValue> {
    opfs_call("store", db_name, Some(&entries)).await?;
    Ok(())
}
