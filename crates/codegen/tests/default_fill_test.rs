use forgedb_codegen::{FillValue, RustGenerator, default_fill};
use forgedb_parser::Parser;

fn parse(src: &str) -> forgedb_parser::Schema {
    Parser::new(src)
        .and_then(|mut p| p.parse())
        .unwrap_or_else(|e| panic!("fixture schema must parse: {e}\n{src}"))
}

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

#[test]
fn the_scenario_4_fixtures_literal_is_what_default_fill_produces() {
    assert_eq!(
        json_of("string @default(\"pending\")").as_deref(),
        Some("\"pending\""),
        "tests/migrate_answers_test.rs scenario 4 bakes this literal into its \
         transformer-route driver by hand; update both together"
    );
}

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

    let src = "enum Status { Draft, Published }\n\n\
               Thing {\n  id: +uuid\n  value: Status @default(\"Nope\")\n}\n";
    let schema = parse(src);
    assert_eq!(default_fill(&schema, &schema.models[0].fields[1]), None);
}

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

fn dense(body: &str) -> String {
    body.chars().filter(|c| !c.is_whitespace()).collect()
}

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
    assert!(
        d.contains("plain_col.append_string(\"\")"),
        "a field with no @default still backfills the type zero. Body:\n{body}"
    );
}
