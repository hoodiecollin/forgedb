//! A content-addressed parse cache.
//!
//! # Why the key is the source text, not the schema
//!
//! The obvious key is "the schema fixture this came from". It is wrong. `RustGenerator`,
//! `ApiGenerator`, `WasmGenerator`, `FfiGenerator`, `Pyo3Generator`, `NapiGenerator`,
//! `RustSdkGenerator`, `TransformGenerator` and the engine hop all consume the *same*
//! schema and emit *different* Rust. Keying on the schema would hand a caller the wrong
//! tree — silently, because every one of those trees is a valid `syn::File`.
//!
//! Keying on the **content** is correct by construction: two different generators produce
//! two different strings and cannot collide, identical output from any source dedupes, and
//! handwritten repo files go through the same door with no special case.
//!
//! # Why the parse cache is thread-local and the source cache is not
//!
//! **`syn::File` is neither `Send` nor `Sync`.** With the `proc-macro` feature active —
//! which it is here, because `crates/codegen` enables it and cargo unifies features across
//! the graph — `proc_macro2` delegates to the real `proc_macro` bridge, whose `Span`,
//! `Symbol`, `TokenStream` and `TokenTree` are all thread-bound. A
//! `static Mutex<HashMap<_, Arc<syn::File>>>` does not compile, and no amount of
//! `default-features = false` on this crate fixes it, because the feature is unified in
//! from a sibling.
//!
//! This was found by compiling, not by reading: the design it replaces looked entirely
//! reasonable on paper.
//!
//! So the cache is split:
//!
//! * **Source text** — `String` is `Send + Sync`, so generated code is memoized in a
//!   process-wide map. This is the larger prize: `RustGenerator::generate` is called 107
//!   times in one test binary at ~184 ms each in debug, roughly 20 s of pure regeneration
//!   the suite already pays today.
//! * **Parsed trees** — memoized per thread via `thread_local!`. Each test thread parses a
//!   given source at most once. With N test threads that is at most N parses instead of
//!   193, which recovers most of the win without pretending a non-`Send` type is shareable.
//!
//! # The limit, stated so nobody is surprised
//!
//! Both caches are per-process, and the parse cache is additionally per-thread.
//! `codegen_snapshots` is one test binary, so its 193 tests share the source cache; each
//! integration test under `tests/` is a separate binary and shares nothing. That is fine —
//! those scan small handwritten files, not 262 KB blobs — but it bounds the win.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

/// Hit/miss counters, so a test can prove the cache is actually being used rather than
/// trusting that it is. A cache that silently never hits looks exactly like a slow suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}

/// FNV-1a over the source bytes.
///
/// Deliberately NOT `DefaultHasher`: this workspace has already been bitten by
/// `DefaultHasher` not being stable across Rust releases. Nothing here is persisted, so
/// stability is not strictly required today — but a cache key that silently changes
/// meaning between toolchains is the kind of thing that gets persisted later by someone
/// who did not read this comment.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Parsed trees — thread-local, because `syn::File` is not `Send`.
// ---------------------------------------------------------------------------

thread_local! {
    static PARSED: RefCell<HashMap<u64, Rc<syn::File>>> = RefCell::new(HashMap::new());
    static PARSE_STATS: RefCell<CacheStats> = const { RefCell::new(CacheStats { hits: 0, misses: 0 }) };
}

/// Parse `src`, reusing a previously parsed tree for identical content **on this thread**.
///
/// # Panics
///
/// Panics if `src` does not parse as Rust. That is intentional: for generated code it
/// cannot happen — nine generators already round-trip their output through `syn` and
/// hard-fail before returning, so unparseable output never escapes generation. For a
/// handwritten repo file it means the file is genuinely broken, which a test should shout
/// about rather than skip.
pub fn cached_parse(src: &str) -> Rc<syn::File> {
    let key = fnv1a(src.as_bytes());

    if let Some(hit) = PARSED.with(|c| c.borrow().get(&key).cloned()) {
        PARSE_STATS.with(|s| s.borrow_mut().hits += 1);
        return hit;
    }

    // Parse OUTSIDE any borrow. Parsing a 262 KB file takes ~144 ms in debug, and holding a
    // `RefCell` borrow across it would risk a re-entrant borrow panic if a future query
    // ever parses lazily.
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

/// Parse-cache hit/miss counts **for the calling thread**.
pub fn cache_stats() -> CacheStats {
    PARSE_STATS.with(|s| *s.borrow())
}

// ---------------------------------------------------------------------------
// Generated source — process-wide, because `String` IS `Send + Sync`, and because
// regeneration is the more expensive half.
// ---------------------------------------------------------------------------

type SourceMap = Mutex<HashMap<u64, &'static str>>;

fn source_cache() -> &'static SourceMap {
    static C: OnceLock<SourceMap> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Memoize an expensive source-producing closure under `key`, process-wide.
///
/// `generate` runs at most once per distinct key per process, however many threads race
/// for it. Intended for `RustGenerator::generate(&schema)` and friends, where the key is
/// something like `("rust", schema_src)`.
///
/// The value is leaked to `&'static str` on first insert. That is deliberate and bounded:
/// entries live for the whole process anyway (a test binary), the count is the number of
/// distinct fixture/generator pairs, and it lets callers hold a plain `&'static str`
/// without a guard or an `Arc` — which matters because the returned text is fed straight
/// into `RustSource`, whose tree is thread-local.
pub fn cached_source(key: &str, generate: impl FnOnce() -> String) -> &'static str {
    let k = fnv1a(key.as_bytes());

    if let Some(hit) = source_cache().lock().expect("source cache poisoned").get(&k) {
        return hit;
    }

    // Generate outside the lock — this is the ~184 ms call, and serializing every thread
    // behind it would undo the point of the cache.
    let produced: &'static str = Box::leak(generate().into_boxed_str());

    let mut map = source_cache().lock().expect("source cache poisoned");
    // A racing thread may have inserted first. Keep the existing entry so every caller sees
    // one pointer; the loser's `produced` stays leaked, which is a bounded one-off.
    map.entry(k).or_insert(produced)
}
