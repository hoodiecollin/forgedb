use std::path::Path;

use forgedb_source_guard::{module_prefix_of, DocAttr, DocSite, RustSource};

fn sites(src: &str, prefix: &[&str]) -> Vec<DocSite> {
    let prefix: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
    RustSource::generated("fixture.rs", src).doc_sites(&prefix)
}

fn documented(src: &str, prefix: &[&str]) -> Vec<(String, DocAttr)> {
    sites(src, prefix)
        .into_iter()
        .filter_map(|s| s.attr.map(|a| (s.key, a)))
        .collect()
}

fn keys(src: &str, prefix: &[&str]) -> Vec<String> {
    sites(src, prefix).into_iter().map(|s| s.key).collect()
}

#[test]
fn a_doc_comment_block_is_one_literal_site_keyed_on_the_item() {
    let got = documented("/// a\n/// b\npub fn f() {}\n", &[]);
    assert_eq!(got, vec![("f".to_string(), DocAttr::Literal("a\nb".to_string()))]);
}

#[test]
fn an_include_str_attribute_on_an_inherent_method_is_keyed_type_dot_method() {
    let src = r#"
pub struct Foo;
impl Foo {
    #[doc = include_str!("../docs/Foo.bar.md")]
    pub fn bar(&self) {}
    fn private(&self) {}
}
"#;
    let got = documented(src, &[]);
    assert_eq!(
        got,
        vec![(
            "Foo.bar".to_string(),
            DocAttr::IncludeStr("../docs/Foo.bar.md".to_string())
        )]
    );
    assert_eq!(keys(src, &[]), vec!["crate", "Foo", "Foo.bar"]);
}

#[test]
fn a_wasm32_gated_item_is_not_an_attachment_point_but_the_host_branch_is() {
    let src = r#"
#[cfg(target_arch = "wasm32")]
pub struct W { pub f: u8 }
#[cfg(not(target_arch = "wasm32"))]
pub struct H { pub f: u8 }
"#;
    assert_eq!(keys(src, &[]), vec!["crate", "H", "H.f"]);
}

#[test]
fn nested_modules_extend_the_key_and_private_fields_are_skipped() {
    let src = r#"
pub mod a {
    pub struct B { pub c: u8, hidden: u8 }
}
mod private { pub struct P; }
"#;
    assert_eq!(keys(src, &[]), vec!["crate", "a", "a.B", "a.B.c"]);
}

#[test]
fn variants_and_their_named_fields_are_sites_and_positional_fields_are_not() {
    let src = r#"
pub enum E {
    V { f: u8 },
    T(u8),
}
pub struct Tuple(pub u64);
"#;
    assert_eq!(
        keys(src, &[]),
        vec!["crate", "E", "E.V", "E.V.f", "E.T", "Tuple"]
    );
}

#[test]
fn the_crate_root_is_keyed_crate_and_a_module_file_is_keyed_by_its_prefix() {
    let src = "#![doc = include_str!(\"../docs/crate.md\")]\npub fn f() {}\n";
    let root = documented(src, &[]);
    assert_eq!(
        root,
        vec![(
            "crate".to_string(),
            DocAttr::IncludeStr("../docs/crate.md".to_string())
        )]
    );
    assert_eq!(keys(src, &["durable"]), vec!["durable", "durable.f"]);
}

#[test]
fn a_trait_impl_is_not_an_attachment_point_but_a_trait_definition_is() {
    let src = "pub struct Foo;\n\
        impl std::fmt::Display for Foo {\n\
            /// not a site\n\
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }\n\
        }\n\
        pub trait T {\n\
            /// m\n\
            fn m(&self);\n\
            const C: u8;\n\
        }\n";
    assert_eq!(keys(src, &[]), vec!["crate", "Foo", "T", "T.m", "T.C"]);
    assert_eq!(
        documented(src, &[]),
        vec![("T.m".to_string(), DocAttr::Literal("m".to_string()))]
    );
}

#[test]
fn a_doc_literal_beside_an_include_reads_as_literal_and_two_includes_read_as_literal() {
    let both = "#[doc = \"x\"]\n#[doc = include_str!(\"../docs/f.md\")]\npub fn f() {}\n";
    assert!(matches!(
        documented(both, &[]).as_slice(),
        [(k, DocAttr::Literal(_))] if k == "f"
    ));
    let twice = "#[doc = include_str!(\"a.md\")]\n#[doc = include_str!(\"b.md\")]\npub fn f() {}\n";
    assert!(matches!(
        documented(twice, &[]).as_slice(),
        [(k, DocAttr::Literal(_))] if k == "f"
    ));
    let hidden = "#[doc(hidden)]\npub fn f() {}\n";
    assert!(documented(hidden, &[]).is_empty());
}

#[test]
fn a_site_reports_the_line_its_item_starts_on_including_its_attributes() {
    let src = "pub struct S;\n\n#[derive(Debug)]\npub struct T {\n    pub f: u8,\n}\n";
    let lines: Vec<(String, usize)> = sites(src, &[]).into_iter().map(|s| (s.key, s.line)).collect();
    assert_eq!(
        lines,
        vec![
            ("crate".to_string(), 1),
            ("S".to_string(), 1),
            ("T".to_string(), 3),
            ("T.f".to_string(), 5),
        ]
    );
}

#[test]
fn module_prefix_comes_from_the_file_path_under_src() {
    assert_eq!(module_prefix_of(Path::new("src/lib.rs")), Vec::<String>::new());
    assert_eq!(module_prefix_of(Path::new("src/durable.rs")), vec!["durable"]);
    assert_eq!(module_prefix_of(Path::new("src/a/mod.rs")), vec!["a"]);
    assert_eq!(module_prefix_of(Path::new("src/a/b.rs")), vec!["a", "b"]);
}
