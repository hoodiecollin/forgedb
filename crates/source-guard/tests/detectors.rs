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
fn an_or_expression_joining_a_literal_and_an_ident_is_found_in_either_order() {
    let planted = RustSource::generated(
        "planted.rs",
        r#"
fn a(f: &Field) -> bool { f.name == "id" || f.auto_generate }
fn b(f: &Field) -> bool { f.auto_generate || f.name == "id" }
fn c(f: &Field) -> bool { f.name == "id" || f.unique }
fn d(f: &Field) -> bool { f.name == "other" || f.auto_generate }
fn e(f: &Field) -> bool { assert!(f.name == "id" || f.auto_generate); true }
"#,
    );
    let hits = planted.or_expressions_joining("id", "auto_generate");
    assert_eq!(hits.len(), 3, "a, b and the one inside assert!: {hits:?}");
    assert!(planted.or_expressions_joining("id", "nothing").is_empty());
}

#[test]
fn identifiers_containing_a_substring_are_counted_in_items_and_macro_bodies() {
    let planted = RustSource::generated(
        "planted.rs",
        "fn PyInit__forgedb_native() {}\nfn other() { assert!(_forgedb_native_ok()); }\nconst STEM: &str = \"_forgedb_native\";\n",
    );
    assert_eq!(planted.idents_containing("_forgedb_native"), 2);
    assert_eq!(planted.string_literals().iter().filter(|s| s.contains("_forgedb_native")).count(), 1);
}

#[test]
fn the_token_rendering_carries_code_and_not_prose() {
    let planted = RustSource::generated(
        "planted.rs",
        "/// Answer lives here in prose only\n// and CopyField here\n#[tokio::main]\nfn run() { let x = Vec::<Step>::new(); let _ = x; }\n",
    );
    let code = planted.tokens_without_docs();
    assert!(!code.contains("Answer"), "a doc comment must not survive: {code}");
    assert!(!code.contains("CopyField"), "a line comment must not survive: {code}");
    assert!(code.contains("Step"), "code must survive: {code}");
    assert!(planted.uses().contains("tokio"), "an attribute path root counts as a use");
}

#[test]
fn a_negated_contains_is_counted_and_a_plain_or_bare_assert_is_not() {
    assert_eq!(src().negated_method_call_count("contains"), 3);
    assert_eq!(src().negated_method_call_count("lines"), 0);
}
