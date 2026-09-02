use forgedb_source_guard::{Marker, RustSource};

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
    let s = scope(CORRECT);
    let f = s.fn_named("list").unwrap();
    assert!(
        f.position(Marker::LetBinding("__sel")).is_some(),
        "a type-annotated binding must still be located"
    );
}

#[test]
fn order_sees_into_nested_blocks() {
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
    let s = RustSource::generated(
        "prose.rs",
        r#"
pub fn list() -> Vec<Row> {
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
