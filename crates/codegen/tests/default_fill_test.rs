//! `@default` has exactly ONE lowering, and both routes derive from it (#374
//! step 4).
//!
//! A newly-added required field reaches existing rows two ways — the generated
//! reopen backfill (`recover_from_wal`) and the offline transformer's hop — and
//! before this they disagreed: the backfill wrote the type zero
//! unconditionally, the hop wrote the recorded default. `status: string
//! @default("pending")` produced `""` in one dir and `"pending"` in the other
//! for the same schema edit, decided by which command the operator ran.
//!
//! What is asserted here is tier 1 and structural: that `default_fill` resolves
//! what it claims to, and that the **generated backfill reaches it**. That the
//! two routes then produce *equal rows* is a claim about running code, and is
//! asserted by running it — `tests/migrate_answers_test.rs`, scenario 4.

use forgedb_codegen::{FillValue, RustGenerator, default_fill};
use forgedb_parser::Parser;

fn parse(src: &str) -> forgedb_parser::Schema {
    Parser::new(src)
        .and_then(|mut p| p.parse())
        .unwrap_or_else(|e| panic!("fixture schema must parse: {e}\n{src}"))
}

/// Resolve `Thing.value`'s `@default` in a one-field fixture.
fn fill_of(field_decl: &str) -> Option<FillValue> {
    let src = format!("Thing {{\n  id: +uuid\n  value: {field_decl}\n}}\n");
    let schema = parse(&src);
    let f = schema.models[0]
        .fields
        .iter()
        .find(|f| f.name == "value")
        .expect("the fixture declares `value`");
    default_fill(&schema, f)
}

/// The JSON literal, or `None`.
fn json_of(field_decl: &str) -> Option<String> {
    fill_of(field_decl).map(|f| f.json_literal())
}

#[test]
fn a_resolvable_default_lowers_to_exactly_one_json_literal() {
    let cases: &[(&str, &str)] = &[
        ("bool @default(\"true\")", "true"),
        ("bool @default(\"false\")", "false"),
        ("u32 @default(7)", "7"),
        ("u64 @default(7)", "7"),
        ("i32 @default(-7)", "-7"),
        ("i64 @default(-7)", "-7"),
        ("f64 @default(\"1.5\")", "1.5"),
        // An integral float still renders with a fraction, so every reader
        // decodes it as a float rather than as an integer.
        ("f64 @default(2)", "2.0"),
        ("string @default(\"pending\")", "\"pending\""),
        ("json @default(\"{}\")", "{}"),
        ("json @default(\"plain\")", "\"plain\""),
        ("decimal @default(\"1.25\")", "\"1.25\""),
    ];
    for (decl, expected) in cases {
        assert_eq!(
            json_of(decl).as_deref(),
            Some(*expected),
            "`value: {decl}` must lower to {expected}"
        );
    }
}

/// The exact literal `tests/migrate_answers_test.rs`'s scenario-4 driver bakes.
///
/// That driver is a `const &str` a subprocess compiles, so the literal cannot be
/// computed there. Pinning it here is what keeps the two in step: change the
/// lowering and this fails, naming the driver.
#[test]
fn the_scenario_4_fixtures_literal_is_what_default_fill_produces() {
    assert_eq!(
        json_of("string @default(\"pending\")").as_deref(),
        Some("\"pending\""),
        "tests/migrate_answers_test.rs scenario 4 bakes this literal into its \
         transformer-route driver by hand; update both together"
    );
}

/// An enum carries **both** halves, and neither is derived from the other.
#[test]
fn an_enum_default_carries_the_name_and_the_positional_byte() {
    let src = "enum Status { Draft, Published, Archived }\n\n\
               Thing {\n  id: +uuid\n  value: Status @default(\"Published\")\n}\n";
    let schema = parse(src);
    let f = &schema.models[0].fields[1];
    assert_eq!(
        default_fill(&schema, f),
        Some(FillValue::Enum {
            variant: "Published".to_string(),
            discriminant: 1,
        }),
        "the stored byte is the DECLARATION POSITION and the JSON form is the \
         name; the two routes need different ones"
    );
    assert_eq!(
        default_fill(&schema, f).unwrap().json_literal(),
        "\"Published\""
    );
}

/// The refusals. Each returns `None`, which makes the add `Authored` — so the
/// operator is asked rather than handed an invented encoding or a silent zero.
#[test]
fn an_unresolvable_default_is_none_rather_than_a_substituted_zero() {
    let refused: &[(&str, &str)] = &[
        ("u32 @default(\"x\")", "a non-numeric literal on an integer"),
        ("u32 @default(-1)", "a negative on an unsigned"),
        ("i32 @default(3000000000)", "out of range for i32"),
        ("string? @default(\"pending\")", "nullable: its zero is None"),
        ("timestamp @default(0)", "no settled default spelling"),
        ("uuid @default(\"x\")", "no settled default spelling"),
        ("bytes(4) @default(\"ab\")", "no settled default spelling"),
        ("string(8) @default(\"hi\")", "no settled default spelling"),
        ("bool @default(\"yes\")", "not a bool literal"),
        ("string", "no @default at all"),
    ];
    for (decl, why) in refused {
        assert_eq!(
            json_of(decl),
            None,
            "`value: {decl}` must not resolve ({why}) — `None` routes it to the \
             prompt, a substituted zero routes it to wrong data"
        );
    }

    // A named variant that is not declared is refused rather than guessed at.
    let src = "enum Status { Draft, Published }\n\n\
               Thing {\n  id: +uuid\n  value: Status @default(\"Nope\")\n}\n";
    let schema = parse(src);
    assert_eq!(default_fill(&schema, &schema.models[0].fields[1]), None);
}

/// The body of `recover_from_wal` in generated `database.rs`, **comments
/// stripped**, scoped to the one function — panicking if it is not there.
///
/// Both halves matter. Unscoped, the whole file contains `"pending"` (it is a
/// `@default`, so it may appear in a doc comment or a validator), so the
/// assertion would pass without the backfill arm existing at all. Uncommented,
/// the assertion could be satisfied by the sentence explaining what the arm
/// does. And a `find(...).unwrap_or(0)` here would degrade to the whole file on
/// a rename, which is the same failure wearing a subtler hat.
fn recover_body(code: &str) -> String {
    let start = code
        .find("fn recover_from_wal")
        .expect("generated database.rs must contain `fn recover_from_wal`");
    let rest = &code[start..];
    let end = rest
        .find("\n    }\n")
        .expect("`recover_from_wal` must have a closing brace");
    rest[..end]
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same body with all whitespace removed, because `prettyplease` wraps a
/// method chain across lines and the assertions below are about the CALL, not
/// about how it was formatted.
fn dense(body: &str) -> String {
    body.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The generated reopen backfill **reaches** `default_fill`.
///
/// `default_fill` being right is necessary and not sufficient — the emitter has
/// to call it. Verified by mutation: deleting the `if let Some(fill) = ...`
/// block at the top of `generate_backfill_appends` (the CALL SITE, not the
/// function) makes this test RED with the type-zero append.
#[test]
fn the_reopen_backfill_writes_the_default_and_not_the_type_zero() {
    let schema = parse(
        "Post {\n  id: +uuid\n  title: string\n  status: string @default(\"pending\")\n  \
         plain: string\n}\n",
    );
    let code = RustGenerator::generate(&schema).unwrap().code;
    let body = recover_body(&code);
    let d = dense(&body);

    assert!(
        d.contains("status_col.append_string(\"pending\")"),
        "the backfill must write the resolved @default. Body:\n{body}"
    );
    assert!(
        !d.contains("status_col.append_string(\"\")"),
        "the type zero must NOT also be emitted for a defaulted field. Body:\n{body}"
    );
    // The control: a field with no `@default` still backfills its type zero, so
    // the assertion above is about the directive and not about strings in
    // general.
    assert!(
        d.contains("plain_col.append_string(\"\")"),
        "a field with no @default still backfills the type zero. Body:\n{body}"
    );
}
