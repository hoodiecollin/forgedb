use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub trait LazySource {
    fn len(&self, path: &Path) -> Option<usize>;
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
}

pub struct DirtyColumn {
    pub path: PathBuf,
    pub offset: usize,
    pub bytes: Vec<u8>,
    pub truncate: bool,
}

thread_local! {
    static STORE: RefCell<HashMap<PathBuf, Vec<u8>>> = RefCell::new(HashMap::new());
    static LOADED: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
    static SOURCE: RefCell<Option<Box<dyn LazySource>>> = const { RefCell::new(None) };
    static COMMITTED_LEN: RefCell<HashMap<PathBuf, usize>> = RefCell::new(HashMap::new());
    static META: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
}

pub fn set_source(source: Box<dyn LazySource>) {
    SOURCE.with(|s| *s.borrow_mut() = Some(source));
}

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

pub(crate) fn ensure(path: &Path) {
    if LOADED.with(|l| l.borrow().contains(path)) {
        return;
    }
    let backed = SOURCE.with(|s| s.borrow().as_ref().and_then(|src| src.len(path)).is_some());
    if backed {
        return;
    }
    STORE.with(|s| {
        s.borrow_mut().entry(path.to_path_buf()).or_default();
    });
    LOADED.with(|l| {
        l.borrow_mut().insert(path.to_path_buf());
    });
}

pub(crate) fn with_bytes<R>(path: &Path, f: impl FnOnce(&[u8]) -> R) -> R {
    ensure_loaded(path);
    STORE.with(|s| {
        let map = s.borrow();
        let empty: &[u8] = &[];
        f(map.get(path).map(Vec::as_slice).unwrap_or(empty))
    })
}

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

pub(crate) fn byte_len(path: &Path) -> usize {
    if LOADED.with(|l| l.borrow().contains(path)) {
        return STORE.with(|s| s.borrow().get(path).map(Vec::len).unwrap_or(0));
    }
    if let Some(n) = SOURCE.with(|s| s.borrow().as_ref().and_then(|src| src.len(path))) {
        return n;
    }
    STORE.with(|s| s.borrow().get(path).map(Vec::len).unwrap_or(0))
}

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

pub fn dump() -> Vec<(PathBuf, Vec<u8>)> {
    STORE.with(|s| {
        s.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    })
}

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

pub fn mark_committed(path: &Path, len: usize) {
    COMMITTED_LEN.with(|c| {
        c.borrow_mut().insert(path.to_path_buf(), len);
    });
}

pub fn clear() {
    STORE.with(|s| s.borrow_mut().clear());
    LOADED.with(|l| l.borrow_mut().clear());
    SOURCE.with(|s| *s.borrow_mut() = None);
    COMMITTED_LEN.with(|c| c.borrow_mut().clear());
    META.with(|m| m.borrow_mut().clear());
}

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

pub fn get(path: &Path) -> Option<Vec<u8>> {
    ensure_loaded(path);
    STORE.with(|s| s.borrow().get(path).cloned())
}
