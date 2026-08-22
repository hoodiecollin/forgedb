//! Guards for the ordering queries.
//!
//! The headline acceptance case is `0c9b802` — the mutation that defeated the original
//! #281 guard. Everything else here is the failure modes around it.

use forgedb_source_guard::{Marker, RustSource};

/// The shape #281 guards: hoist a predicate, gate a fast page on it, and only then probe
/// the index on the scan path.
const CORRECT: &str = r#"
pub fn list() -> Vec<Row> {
    let __keep_all: bool = __post_is_unfiltered(&params);
    if __keep_all && qp.sort.is_none() {
        return db.post.__with_fast_page(qp);
    }
    let __sel: Option<Vec<usize>> = __rows_by_views(&params);
    return db.post.__with_page(__sel, qp);
}
"#;

/// The `0c9b802` mutation, verbatim in shape: the pushdown is resolved into an EARLIER
/// binding and merely rebound at the old site. The *name* `__sel` never moves, so a guard
/// anchored on `let __sel` still sees it exactly where it was — while the actual index
/// probe has migrated above the fast-path branch, which is the waste the ordering exists
/// to prevent.
const REBOUND: &str = r#"
pub fn list() -> Vec<Row> {
    let __sel_early: Option<Vec<usize>> = __rows_by_views(&params);
    let __keep_all: bool = __post_is_unfiltered(&params);
    if __keep_all && qp.sort.is_none() {
        return db.post.__with_fast_page(qp);
    }
    let __sel: Option<Vec<usize>> = __sel_early;
    return db.post.__with_page(__sel, qp);
}
"#;

const ORDER: [Marker; 4] = [
    Marker::LetBinding("__keep_all"),
    Marker::Call("__with_fast_page"),
    Marker::Call("__rows_by_views"),
    Marker::Call("__with_page"),
];

fn scope(src: &str) -> RustSource {
    RustSource::generated("list.rs", src)
}

#[test]
fn the_correct_shape_is_in_order() {
    let s = scope(CORRECT);
    let f = s.fn_named("list").unwrap();
    assert!(
        f.in_order(&ORDER),
        "expected keep -> fast -> sel -> page, got {}",
        f.explain_order(&ORDER)
    );
}

#[test]
fn the_0c9b802_rebinding_mutation_is_caught() {
    // THE acceptance case. A byte-offset guard anchored on `let __sel` stays green here.
    let s = scope(REBOUND);
    let f = s.fn_named("list").unwrap();
    assert!(
        !f.in_order(&ORDER),
        "the pushdown moved above the fast-path branch and the guard MUST catch it; got {}",
        f.explain_order(&ORDER)
    );
}

#[test]
fn anchoring_on_the_binding_instead_of_the_call_is_what_fails() {
    // Demonstrates *why* Marker::Call is preferred, rather than asserting it in prose.
    // The binding name `__sel` sits in the same place in both versions, so a guard keyed on
    // it cannot tell them apart — which is exactly the bug 0c9b802 fixed.
    let by_binding = [
        Marker::LetBinding("__keep_all"),
        Marker::Call("__with_fast_page"),
        Marker::LetBinding("__sel"),
        Marker::Call("__with_page"),
    ];

    let correct = scope(CORRECT);
    let rebound = scope(REBOUND);

    assert!(correct.fn_named("list").unwrap().in_order(&by_binding));
    assert!(
        rebound.fn_named("list").unwrap().in_order(&by_binding),
        "a binding-anchored order is satisfied by BOTH shapes — this passing is the \
         defect, and is why guards anchor on the call"
    );
}

#[test]
fn a_missing_marker_is_not_vacuously_ordered() {
    // An ordering claim about something absent is unanswerable, not satisfied. Answering
    // `true` here is how a guard goes silently vacuous when an anchor rots.
    let s = scope(CORRECT);
    let f = s.fn_named("list").unwrap();
    let with_ghost = [
        Marker::LetBinding("__keep_all"),
        Marker::Call("__does_not_exist"),
    ];
    assert!(!f.in_order(&with_ghost), "absent marker must not order true");
    assert!(
        f.explain_order(&with_ghost).contains("ABSENT"),
        "and the message must say which one is missing: {}",
        f.explain_order(&with_ghost)
    );
}

#[test]
fn an_annotated_let_is_found() {
    // `let __sel: Option<Vec<usize>> = …` parses as Pat::Type wrapping Pat::Ident, not as a
    // bare Pat::Ident. Every annotated binding in the generated code looks like this, so
    // missing that arm would answer None for most real bindings — a silent hole of exactly
    // the kind this crate exists to delete.
    let s = scope(CORRECT);
    let f = s.fn_named("list").unwrap();
    assert!(
        f.position(Marker::LetBinding("__sel")).is_some(),
        "a type-annotated binding must still be located"
    );
}

#[test]
fn order_sees_into_nested_blocks() {
    // A `Block.stmts` index only orders siblings. The real guards compare a top-level `let`
    // against a call inside an `if` branch, which a sibling index reports as absent.
    let s = scope(CORRECT);
    let f = s.fn_named("list").unwrap();
    assert!(
        f.position(Marker::Call("__with_fast_page")).is_some(),
        "a call nested inside an `if` branch must be reachable"
    );
    assert!(
        f.position(Marker::LetBinding("__keep_all")).unwrap()
            < f.position(Marker::Call("__with_fast_page")).unwrap(),
        "and it must order after the top-level binding"
    );
}

#[test]
fn prose_does_not_participate_in_ordering() {
    // A needle in a comment or string is a byte offset like any other, so the old form
    // could be reordered by a doc comment. An AST cannot see either.
    let s = RustSource::generated(
        "prose.rs",
        r#"
pub fn list() -> Vec<Row> {
    // __rows_by_views(&params) happens later, despite this comment being first.
    let note = "__rows_by_views(&params)";
    let __keep_all: bool = f(&params);
    let _ = note;
    __rows_by_views(&params)
}
"#,
    );
    let f = s.fn_named("list").unwrap();
    assert!(
        f.in_order(&[
            Marker::LetBinding("__keep_all"),
            Marker::Call("__rows_by_views"),
        ]),
        "the comment and the string literal must not count as the call: {}",
        f.explain_order(&[
            Marker::LetBinding("__keep_all"),
            Marker::Call("__rows_by_views"),
        ])
    );
}

#[test]
fn formatting_does_not_change_the_verdict() {
    // prettyplease breaks lines by length, so the same call renders differently between
    // schemas — the documented reason the `regex` dev-dep exists.
    let broken = RustSource::generated(
        "broken.rs",
        "pub fn list() { let __keep_all: bool = f();\n    return db\n        .post\n        .__with_page(\n            __sel,\n        ); }",
    );
    let f = broken.fn_named("list").unwrap();
    assert!(
        f.in_order(&[
            Marker::LetBinding("__keep_all"),
            Marker::Call("__with_page")
        ]),
        "a call split across five lines is the same call"
    );
}
