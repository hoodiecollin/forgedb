use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}

pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

thread_local! {
    static PARSED: RefCell<HashMap<u64, Rc<syn::File>>> = RefCell::new(HashMap::new());
    static PARSE_STATS: RefCell<CacheStats> = const { RefCell::new(CacheStats { hits: 0, misses: 0 }) };
}

pub fn cached_parse(src: &str) -> Rc<syn::File> {
    let key = fnv1a(src.as_bytes());

    if let Some(hit) = PARSED.with(|c| c.borrow().get(&key).cloned()) {
        PARSE_STATS.with(|s| s.borrow_mut().hits += 1);
        return hit;
    }

    let parsed = Rc::new(
        syn::parse_file(src).unwrap_or_else(|e| panic!("source-guard: source did not parse: {e}")),
    );

    PARSE_STATS.with(|s| s.borrow_mut().misses += 1);
    PARSED.with(|c| {
        Rc::clone(
            c.borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::clone(&parsed)),
        )
    })
}

pub fn cache_stats() -> CacheStats {
    PARSE_STATS.with(|s| *s.borrow())
}

type SourceMap = Mutex<HashMap<u64, &'static str>>;

fn source_cache() -> &'static SourceMap {
    static C: OnceLock<SourceMap> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cached_source(key: &str, generate: impl FnOnce() -> String) -> &'static str {
    let k = fnv1a(key.as_bytes());

    if let Some(hit) = source_cache().lock().expect("source cache poisoned").get(&k) {
        return hit;
    }

    let produced: &'static str = Box::leak(generate().into_boxed_str());

    let mut map = source_cache().lock().expect("source cache poisoned");
    map.entry(k).or_insert(produced)
}
