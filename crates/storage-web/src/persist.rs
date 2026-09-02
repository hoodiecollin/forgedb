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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    IndexedDb,
    Opfs,
}

impl Backend {
    pub fn from_str_lossy(s: &str) -> Backend {
        match s.to_ascii_lowercase().as_str() {
            "opfs" => Backend::Opfs,
            _ => Backend::IndexedDb,
        }
    }
}

const OBJECT_STORE: &str = "columns";

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

pub async fn commit(backend: Backend, db_name: &str) -> Result<(), JsValue> {
    match backend {
        Backend::IndexedDb => idb_store(db_name, store::dump()).await,
        Backend::Opfs => opfs_commit(db_name).await,
    }
}

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

thread_local! {
    static OPFS_HANDLES: RefCell<HashMap<PathBuf, FileSystemSyncAccessHandle>> =
        RefCell::new(HashMap::new());
    static OPFS_DIR: RefCell<Option<FileSystemDirectoryHandle>> = const { RefCell::new(None) };
}

fn opfs_encode(path: &str) -> String {
    path.replace('%', "%25").replace('/', "%2F")
}
fn opfs_decode(name: &str) -> String {
    name.replace("%2F", "/").replace("%25", "%")
}

fn storage_manager() -> Result<StorageManager, JsValue> {
    if let Some(win) = web_sys::window() {
        return Ok(win.navigator().storage());
    }
    let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
    Ok(global.navigator().storage())
}

async fn opfs_directory(db_name: &str) -> Result<FileSystemDirectoryHandle, JsValue> {
    let root_val = JsFuture::from(storage_manager()?.get_directory()).await?;
    let root: FileSystemDirectoryHandle = root_val.dyn_into()?;
    let opts = FileSystemGetDirectoryOptions::new();
    opts.set_create(true);
    let dir_val = JsFuture::from(root.get_directory_handle_with_options(db_name, &opts)).await?;
    dir_val.dyn_into()
}

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

async fn opfs_open_lazy(db_name: &str) -> Result<(), JsValue> {
    let dir = opfs_directory(db_name).await?;
    OPFS_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
    OPFS_HANDLES.with(|h| h.borrow_mut().clear());

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

async fn opfs_handle_for(db_name: &str, arena_path: &Path) -> Result<FileSystemSyncAccessHandle, JsValue> {
    if let Some(ah) = OPFS_HANDLES.with(|h| h.borrow().get(arena_path).cloned()) {
        return Ok(ah);
    }
    let dir = OPFS_DIR
        .with(|d| d.borrow().clone())
        .ok_or_else(|| JsValue::from_str("OPFS directory not opened"))?;
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

async fn opfs_commit(db_name: &str) -> Result<(), JsValue> {
    if OPFS_DIR.with(|d| d.borrow().is_none()) {
        let dir = opfs_directory(db_name).await?;
        OPFS_DIR.with(|d| *d.borrow_mut() = Some(dir));
    }

    let mut dirty = store::dirty_columns();
    dirty.sort_by_key(|dc| dc.truncate);
    for dc in dirty {
        let ah = opfs_handle_for(db_name, &dc.path).await?;
        if dc.truncate {
            ah.truncate_with_f64(0.0)?;
        }
        let opts = FileSystemReadWriteOptions::new();
        opts.set_at(dc.offset as f64);
        let bytes = dc.bytes;
        ah.write_with_u8_array_and_options(&bytes, &opts)?;
        ah.flush()?;
        store::mark_committed(&dc.path, dc.offset + bytes.len());
    }
    Ok(())
}
