use forgedb_source_guard::go_facts;

const BINDING: &str = r#"package forgedb

/*
#include <stdint.h>
*/
import "C"

// Post is a row.
type Post struct {
	Id    uint64
	Title string
}

type Comment struct {
	Id   uint64
	Post uint64
	Body string
}

//export forgedb_open
func forgedb_open() int32 { return 0 }

//export forgedb_close
func forgedb_close() {}

// not a directive: //export inside prose does not count
func helper() {}
"#;

#[test]
fn exported_symbols_come_from_the_cgo_directive_not_from_prose() {
    let f = go_facts(BINDING);
    assert_eq!(f.exported(), ["forgedb_open", "forgedb_close"]);
}

#[test]
fn struct_fields_are_read_by_name_with_their_declared_type() {
    let f = go_facts(BINDING);
    assert_eq!(f.field_type("Post", "Id"), Some("uint64"));
    assert_eq!(f.field_type("Comment", "Post"), Some("uint64"));
    assert_eq!(f.field_type("Comment", "Body"), Some("string"));
    assert_eq!(f.field_type("Comment", "Nope"), None);
    assert_eq!(f.field_type("Nope", "Id"), None);
    assert_eq!(
        f.field_type("Comment", "Post"),
        f.field_type("Post", "Id"),
        "the FK field and the target key are one type, asked of the AST"
    );
}

#[test]
fn a_file_with_no_structs_or_exports_reports_empty_rather_than_absent() {
    let f = go_facts("package sdk\n\nfunc F() {}\n");
    assert!(f.exported().is_empty());
    assert!(f.struct_fields.is_empty());
}
