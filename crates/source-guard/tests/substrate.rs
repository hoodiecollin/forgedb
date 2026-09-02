use forgedb_source_guard::{cache_stats, cached_parse, cached_source, RustSource};

const SAMPLE: &str = r#"
pub struct User { pub id: String }
impl User {
    pub fn insert(&mut self) { self.wal_write(); }
    pub fn update(&mut self) { self.wal_write(); }
}
"#;

#[test]
fn parses_and_exposes_the_tree() {
    let src = RustSource::generated("sample.rs", SAMPLE);
    assert_eq!(src.ast().items.len(), 2, "one struct + one impl");
    assert_eq!(src.origin(), "sample.rs");
}

#[test]
fn identical_content_hits_the_cache() {
    let unique = format!("{SAMPLE}\n// identical_content_hits_the_cache\n");

    let before = cache_stats();
    let _a = RustSource::generated("a.rs", unique.clone());
    let after_first = cache_stats();
    let _b = RustSource::generated("b.rs", unique);
    let after_second = cache_stats();

    assert_eq!(
        after_first.misses,
        before.misses + 1,
        "first parse of new content must MISS"
    );
    assert_eq!(
        after_second.hits,
        after_first.hits + 1,
        "identical content must HIT — a cache that never hits looks exactly like a slow suite"
    );
    assert_eq!(
        after_second.misses, after_first.misses,
        "the second call must not re-parse"
    );
}

#[test]
fn different_content_does_not_collide() {
    let a = RustSource::generated("a.rs", "pub struct A;");
    let b = RustSource::generated("b.rs", "pub struct B;");

    let name_of = |s: &RustSource| match &s.ast().items[0] {
        syn::Item::Struct(it) => it.ident.to_string(),
        other => panic!("expected a struct, got {other:?}"),
    };

    assert_eq!(name_of(&a), "A");
    assert_eq!(name_of(&b), "B", "distinct content must yield distinct trees");
}

#[test]
fn cached_source_generates_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);

    let key = "cached_source_generates_once";
    let first = cached_source(key, || {
        CALLS.fetch_add(1, Ordering::SeqCst);
        "pub struct Generated;".to_string()
    });
    let second = cached_source(key, || {
        CALLS.fetch_add(1, Ordering::SeqCst);
        "pub struct Generated;".to_string()
    });

    assert_eq!(first, second);
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        1,
        "the expensive closure must run once — this is the ~184ms/call generation the \
         cache exists to eliminate"
    );
}

#[test]
fn reads_a_real_repo_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs");
    let src = RustSource::repo_file(path);
    assert!(
        src.ast().items.len() >= 2,
        "lib.rs should expose at least its modules"
    );
}

#[test]
#[should_panic(expected = "did not parse")]
fn unparseable_source_is_fatal_not_silent() {
    cached_parse("pub fn ( ) ) not rust");
}

#[test]
#[should_panic(expected = "cannot read")]
fn a_missing_file_is_fatal_not_skipped() {
    RustSource::repo_file("/nonexistent/definitely/not/here.rs");
}

#[test]
fn the_escape_hatch_returns_the_original_text() {
    let src = RustSource::generated("sample.rs", SAMPLE);
    assert_eq!(
        src.raw_text_because("testing the hatch itself"),
        SAMPLE,
        "the hatch must hand back the source verbatim, not a reformatted round-trip"
    );
}
