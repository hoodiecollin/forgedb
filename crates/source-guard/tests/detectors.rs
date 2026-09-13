use forgedb_source_guard::RustSource;

const SAMPLE: &str = r#"
fn top_level() {
    let n = "a\nb".lines().count();
    let _ = n;
}

struct S;

impl S {
    fn method(&self, text: &str) -> bool {
        assert!(text.lines().any(|l| l == "a"), "inside a macro");
        let split = text
            .lines()
            .map(str::trim)
            .count();
        split > 0
    }
}

fn negations(code: &str) {
    assert!(!code.contains("banned"));
    assert!(code.contains("allowed"));
    assert!(!(code.contains("parens")));
    let _ = !code.contains("plain");
    assert_eq!(code.contains("eq"), true);
}
"#;

fn src() -> RustSource {
    RustSource::generated("sample.rs", SAMPLE)
}

#[test]
fn method_call_sites_are_attributed_to_the_enclosing_fn_including_inside_macros() {
    let sites = src().method_call_sites("lines");
    assert_eq!(sites.get("top_level"), Some(&1));
    assert_eq!(
        sites.get("method"),
        Some(&2),
        "one call inside assert! and one multi-line chain: {sites:?}"
    );
    assert_eq!(sites.get("negations"), None);
    assert_eq!(sites.values().sum::<usize>(), 3);
}

#[test]
fn a_negated_contains_is_counted_and_a_plain_or_bare_assert_is_not() {
    assert_eq!(src().negated_method_call_count("contains"), 3);
    assert_eq!(src().negated_method_call_count("lines"), 0);
}
