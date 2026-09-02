use forgedb_source_guard::go_facts;

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

#[test]
#[should_panic(expected = "rejected the input")]
fn unparseable_go_is_fatal_not_clean() {
    go_facts("package sdk\nfunc ( ) ) {");
}

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

#[test]
fn a_missing_go_toolchain_is_fatal_not_skipped() {
    const RELAY: &str = "SOURCE_GUARD_GO_ABSENT_CHILD";

    if std::env::var_os(RELAY).is_some() {
        go_facts("package main\n");
        return;
    }

    let empty = std::env::temp_dir().join(format!("sg-no-go-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("scratch dir");

    let out = std::process::Command::new(std::env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "a_missing_go_toolchain_is_fatal_not_skipped",
            "--nocapture",
        ])
        .env(RELAY, "1")
        .env("PATH", &empty)
        .output()
        .expect("re-run this test binary with an empty PATH");

    let _ = std::fs::remove_dir_all(&empty);

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        !out.status.success(),
        "with no `go` on PATH the child must FAIL. It exited {:?} instead, which is the \
         skip-shaped outcome design D6 forbids: a guard that cannot evaluate must never \
         report green.\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status.code()
    );

    assert!(
        stderr.contains("cannot run `go`"),
        "the child failed, but not by the path under test — so this test would keep passing \
         if the toolchain check were deleted. Expected the `cannot run \\`go\\`` panic.\
         \nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(
        !stdout.contains("test result: ok"),
        "the child reported a passing run, which means it SKIPPED rather than failed\
         \nstdout:\n{stdout}"
    );
}
