use forgedb_source_guard::RustSource;

const SAMPLE: &str = r#"
use std::process::Command;
use serde_json::Value;
use utoipa::ToSchema;

pub const WAL_CHECKPOINT_INTERVAL: u64 = 512;
pub const NAME: &str = "app";

#[derive(Debug, Clone, ToSchema)]
pub struct Author {
    pub id: String,
    #[schema(value_type = String)]
    pub email: String,
}

pub struct Post {
    pub id: String,
    pub auto_sequences: u32,
}

pub enum Kind {
    Napi,
    Pyo3,
    Ffi,
}

pub fn spawn() {
    let _c = Command::new("cargo");
    let _r = Command::new("rustc");
    assert!(Command::new("cargo").get_program() == "cargo");
    let m = manifest.auto_sequences;
    let _ = m + manifest.auto_sequences;
    let _v: Value = serde_json::from_str("{}").unwrap();
}

pub fn total(k: Kind) -> u8 {
    match k {
        Kind::Napi => 1,
        Kind::Pyo3 => 2,
        Kind::Ffi => 3,
    }
}

pub fn partial(k: Kind) -> u8 {
    match k {
        Kind::Napi => 1,
        _ if false => 9,
        _ => 0,
    }
}

#[test]
#[ignore = "slow"]
fn slow_case() {}

#[test]
fn fast_case() {
    let text = "a\nb";
    assert!(text.lines().any(|l| l == "a"));
}
"#;

fn src() -> RustSource {
    RustSource::generated("sample.rs", SAMPLE)
}

#[test]
fn a_call_with_a_literal_argument_is_counted_file_wide_including_inside_a_macro() {
    let s = src();
    assert_eq!(s.file_calls_path_with_str_arg("Command::new", "cargo"), 2);
    assert_eq!(s.file_calls_path_with_str_arg("Command::new", "rustc"), 1);
    assert_eq!(s.file_calls_path_with_str_arg("Command::new", "go"), 0);
    assert_eq!(s.file_call_count("new"), 3);
    assert_eq!(s.file_call_count("nowhere"), 0);
}

#[test]
fn a_field_read_is_counted_file_wide_and_a_declaration_is_not_a_read() {
    let s = src();
    assert_eq!(s.file_field_read_count("auto_sequences"), 2);
    assert_eq!(s.structs_declaring_field("auto_sequences"), vec!["Post".to_string()]);
    assert!(s.structs_declaring_field("nothing").is_empty());
}

#[test]
fn identifiers_are_counted_everywhere_including_macro_bodies() {
    let s = src();
    assert_eq!(s.ident_count("manifest"), 2);
    assert!(s.ident_count("lines") >= 1, "the call inside assert! must be seen");
    assert_eq!(s.ident_count("absent_ident"), 0);
    assert_eq!(s.free_fn_names(), vec!["spawn", "total", "partial", "slow_case", "fast_case"]);
}

#[test]
fn use_roots_include_use_items_and_expression_paths() {
    let s = src();
    let uses = s.uses();
    for want in ["std", "serde_json", "utoipa"] {
        assert!(uses.contains(want), "missing {want} in {uses:?}");
    }
    assert!(!uses.contains("Command"), "a type is not a crate root: {uses:?}");
    assert!(!uses.contains("Kind"), "a local enum path is not a crate root: {uses:?}");
}

#[test]
fn derives_and_attributes_are_read_from_the_ast() {
    let s = src();
    assert_eq!(s.derives("Author").unwrap(), vec!["Debug", "Clone", "ToSchema"]);
    assert!(s.derives("Post").unwrap().is_empty());
    assert!(s.derives("Nope").is_err());
    assert!(s.any_derive("ToSchema"));
    assert!(!s.any_derive("Serialize"));
    assert_eq!(s.attr_count("schema"), 1);
    assert_eq!(s.attr_count("ignore"), 1);
    assert_eq!(s.fns_with_attr("ignore"), vec!["slow_case"]);
    assert_eq!(s.fns_with_attr("test"), vec!["slow_case", "fast_case"]);

    let stripped = RustSource::generated("stripped.rs", SAMPLE.replace(", ToSchema", ""));
    assert!(!stripped.any_derive("ToSchema"), "removing the derive must be visible");
}

#[test]
fn a_struct_fields_attributes_are_read_by_name_and_a_miss_names_the_fields() {
    let s = src();
    assert_eq!(s.field_attrs("Author", "email").unwrap(), vec!["schema(value_type = String)"]);
    assert!(s.field_attrs("Author", "id").unwrap().is_empty());
    let err = s.field_attrs("Author", "nope").unwrap_err().to_string();
    assert!(err.contains("email"), "lists the fields present: {err}");
    assert!(s.field_attrs("Nope", "id").is_err());
}

#[test]
fn a_const_is_read_by_name_and_a_miss_names_what_exists() {
    let s = src();
    assert_eq!(s.const_u64("WAL_CHECKPOINT_INTERVAL").unwrap(), 512);
    assert_eq!(s.const_expr("NAME").unwrap(), "\"app\"");
    let err = s.const_u64("MISSING").unwrap_err().to_string();
    assert!(err.contains("WAL_CHECKPOINT_INTERVAL"), "lists the consts present: {err}");
    assert!(s.const_u64("NAME").is_err(), "a string const is not a u64");
}

#[test]
fn a_match_reports_its_wildcard_arm_and_its_variant_paths() {
    let s = src();
    let total = s.fn_named("total").unwrap();
    assert!(!total.match_has_wildcard_arm());
    let paths = total.match_arm_paths();
    for want in ["Kind::Napi", "Kind::Pyo3", "Kind::Ffi"] {
        assert!(paths.iter().any(|p| p == want), "missing {want} in {paths:?}");
    }
    let partial = s.fn_named("partial").unwrap();
    assert!(partial.match_has_wildcard_arm());
}

#[test]
fn string_literals_are_collected_from_items_and_macro_bodies() {
    let s = src();
    let lits = s.string_literals();
    for want in ["cargo", "rustc", "app", "{}", "a\nb", "slow"] {
        assert!(lits.iter().any(|l| l == want), "missing {want:?} in {lits:?}");
    }
}

#[test]
fn walking_a_directory_yields_every_rust_file_and_skips_named_dirs() {
    let tmp = std::env::temp_dir().join(format!("source-guard-walk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("a/target")).unwrap();
    std::fs::create_dir_all(tmp.join("b")).unwrap();
    std::fs::write(tmp.join("a/one.rs"), "fn one() {}").unwrap();
    std::fs::write(tmp.join("a/target/skip.rs"), "fn skip() {}").unwrap();
    std::fs::write(tmp.join("b/two.rs"), "fn two() {}").unwrap();
    std::fs::write(tmp.join("b/notes.md"), "fn not_rust() {}").unwrap();

    let files = RustSource::walk(&tmp, &["target"]);
    let names: Vec<String> = files
        .iter()
        .flat_map(|f| f.free_fn_names())
        .collect();
    assert_eq!(names, vec!["one", "two"]);
    let _ = std::fs::remove_dir_all(&tmp);
}
