//! The identity red line, and the two directions the five substrings failed in.
//!
//! These are the acceptance criteria for the Go half. The old guard was:
//!
//! ```ignore
//! for forbidden in ["forgedb_query", "switch model", "predicate", "QueryBuilder", "reflect."] {
//!     assert!(!code.contains(forbidden));
//! }
//! ```
//!
//! Each test below names which direction it covers.

use forgedb_source_guard::go_facts;

/// FALSE ALARM. Every one of the five substrings appears here, all of it prose or string
/// data. The old guard reported three hits; there is no violation.
#[test]
fn innocent_prose_does_not_trip_the_red_line() {
    let f = go_facts(
        r#"package sdk

// Find returns rows. It builds no predicate and is not a QueryBuilder;
// it does not use reflect. at all, and never touches forgedb_query.
func Find() string {
	return "predicate QueryBuilder reflect. forgedb_query switch model"
}
"#,
    );

    assert!(!f.imports("reflect"), "a comment is not an import");
    assert!(
        !f.dispatches_generically(),
        "a string literal is not a switch: {:?}",
        f.string_switch_tags
    );
}

/// FALSE GREEN — the dangerous direction. A real generic runtime query surface, spelled to
/// dodge all five needles: `reflect` aliased to `rt`, and the tag renamed from `model` to
/// `kind`. The old guard passed this.
#[test]
fn the_properly_evasive_violation_is_caught() {
    let f = go_facts(
        r#"package sdk

import rt "reflect"

type Matcher struct{}
type Account struct{}
type Project struct{}

func Find(kind string, ms []Matcher) []any {
	switch kind {
	case "Account":
		return scan(rt.TypeOf(Account{}), ms)
	case "Project":
		return scan(rt.TypeOf(Project{}), ms)
	}
	return nil
}

func scan(t any, ms []Matcher) []any { return nil }
"#,
    );

    assert!(
        f.imports("reflect"),
        "the import PATH must be seen through the `rt` alias; got {:?}",
        f.import_paths
    );
    assert_eq!(
        f.import_aliases.get("rt").map(String::as_str),
        Some("reflect"),
        "and the alias itself is recorded, so the failure can explain the spelling"
    );
    assert!(
        f.dispatches_generically(),
        "a switch with string-literal cases IS model routing, whatever the tag is named; \
         got {:?}",
        f.string_switch_tags
    );
}

/// The historical near-miss. This variant DID trip the old guard — but only by luck,
/// because `switch model` and `reflect.` happen to be two of the five literals.
#[test]
fn the_lucky_hit_is_caught_on_purpose_now() {
    let f = go_facts(
        r#"package sdk

import "reflect"

func Find(model string) any {
	switch model {
	case "A":
		return reflect.TypeOf(0)
	}
	return nil
}
"#,
    );
    assert!(f.imports("reflect"));
    assert!(f.dispatches_generically());
}

/// A type switch is generic dispatch with no string tag at all, so a tag-only check misses
/// it entirely.
#[test]
fn a_type_switch_is_also_generic_dispatch() {
    let f = go_facts(
        r#"package sdk

func Find(v any) string {
	switch v.(type) {
	case int:
		return "int"
	}
	return ""
}
"#,
    );
    assert_eq!(f.type_switches, 1);
    assert!(f.dispatches_generically(), "a type switch counts");
}

/// The reason `dispatches_generically` is not "has any switch". Generated cgo wrappers
/// switch on an INTEGER status from the C ABI — nine such switches in a two-model schema.
/// An invariant that fails on those is not a red line, it is noise that gets suppressed.
#[test]
fn an_integer_status_switch_is_legitimate() {
    let f = go_facts(
        r#"package sdk

func Update(db *DB) (bool, error) {
	r := 0
	switch r {
	case 1:
		return true, nil
	case 0:
		return false, nil
	default:
		return false, nil
	}
}
"#,
    );
    assert_eq!(f.switch_tags, vec!["r"], "the switch is seen");
    assert!(
        !f.dispatches_generically(),
        "…but an integer status switch is NOT model routing, and banning it would make the \
         red line unusable: {:?}",
        f.string_switch_tags
    );
}

/// A guard that cannot evaluate must not report clean. An empty verdict and a clean verdict
/// are indistinguishable otherwise.
#[test]
#[should_panic(expected = "rejected the input")]
fn unparseable_go_is_fatal_not_clean() {
    go_facts("package sdk\nfunc ( ) ) {");
}

/// The verdict must show it actually saw the file, so a caller can assert the parse
/// happened rather than trusting an empty result.
#[test]
fn the_verdict_proves_the_parse_happened() {
    let f = go_facts(
        r#"package sdk

import "net/http"

type Client struct{ base string }

func (c *Client) GetAccount(id string) error {
	_ = http.MethodGet
	return nil
}
"#,
    );
    assert!(f.decl_count > 0);
    assert!(f.func_names.iter().any(|n| n == "GetAccount"));
    assert!(f.declared_types.iter().any(|t| t == "Client"));
    assert!(f.imports("net/http"));
}
