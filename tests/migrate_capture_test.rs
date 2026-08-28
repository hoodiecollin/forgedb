use forgedb::commands::migrate::answers::resolve_answers;
use forgedb::commands::migrate::escape::language_for;
use forgedb::ask::ScriptedPrompt as Scripted;
use forgedb_migrations::{
    Answer, EscapeLanguage, Migration, SchemaChange, SimpleType, hop_answer_status,
};
use tempfile::TempDir;

fn parse(src: &str) -> forgedb_parser::Schema {
    forgedb_parser::Parser::new(src)
        .and_then(|mut p| p.parse())
        .expect("fixture schema parses")
}

const DEST: &str = "Post {\n  id: +uuid\n  title: string\n  slug: string\n  views: string\n}\n";

fn add_slug() -> SchemaChange {
    SchemaChange::AddField {
        model_name: "Post".into(),
        field_name: "slug".into(),
        field_type: SimpleType::Str,
        nullable: false,
        default_json: None,
        answer: None,
    }
}

fn retype_views() -> SchemaChange {
    SchemaChange::ChangeFieldType {
        model_name: "Post".into(),
        field_name: "views".into(),
        old_type: SimpleType::U32,
        new_type: SimpleType::Str,
        answer: None,
    }
}

#[test]
fn scenario_5_a_constant_is_recorded_as_data() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let mut changes = vec![add_slug()];
    let mut ask = Scripted::new(["1", "untitled"]);

    let scaffold = resolve_answers(
        &mut changes,
        &dest,
        EscapeLanguage::TypeScript,
        t.path(),
        "20260808000000",
        Some(&mut ask),
        "unused",
        (1, 2),
    )
    .expect("the scripted operator answers");

    assert!(scaffold.is_none(), "a constant needs no authored file");
    assert!(ask.is_exhausted(), "every scripted answer was consumed");
    assert_eq!(
        changes[0].answer(),
        Some(&Answer::Constant {
            json: "\"untitled\"".to_string()
        }),
        "the answer is a JSON literal, produced by the SAME conversion `@default` \
         goes through — an operator's answer and a schema's default must not be \
         encoded differently"
    );

    let m = Migration::with_id("20260808000000".into(), "x".into(), changes, 1, 2);
    assert_eq!(m.record_version, 1);
    assert!(m.verify_checksum());
    assert!(m.unanswered().is_empty());
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}

#[test]
fn a_copy_answer_names_a_field_of_the_same_type() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let mut changes = vec![add_slug()];
    let mut ask = Scripted::new(["2", "1"]);

    resolve_answers(
        &mut changes,
        &dest,
        EscapeLanguage::TypeScript,
        t.path(),
        "20260808000000",
        Some(&mut ask),
        "unused",
        (1, 2),
    )
    .unwrap();

    assert_eq!(
        changes[0].answer(),
        Some(&Answer::CopyField {
            field: "title".to_string()
        })
    );
}

#[test]
fn the_copy_option_is_absent_when_nothing_could_be_copied() {
    let t = TempDir::new().unwrap();
    let dest = parse("Post {\n  id: +uuid\n  views: u32\n  slug: string\n}\n");
    let mut changes = vec![add_slug()];
    let mut ask = Scripted::new(["1", "x"]);
    resolve_answers(
        &mut changes,
        &dest,
        EscapeLanguage::Rust,
        t.path(),
        "20260808000000",
        Some(&mut ask),
        "unused",
        (1, 2),
    )
    .unwrap();
    assert_eq!(
        changes[0].answer(),
        Some(&Answer::Constant {
            json: "\"x\"".to_string()
        })
    );
    let mut ask = Scripted::new(["2"]);
    let mut changes = vec![add_slug()];
    resolve_answers(
        &mut changes,
        &dest,
        EscapeLanguage::Rust,
        t.path(),
        "20260808000001",
        Some(&mut ask),
        "unused",
        (1, 2),
    )
    .unwrap();
    assert!(
        matches!(changes[0].answer(), Some(Answer::Escape { .. })),
        "option 2 is the escape hatch when no field could be copied: {:?}",
        changes[0].answer()
    );
}

#[test]
fn the_escape_hatch_records_the_scaffolds_own_hash() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let mut changes = vec![add_slug(), retype_views()];
    let mut ask = Scripted::new(["3", "3"]);

    let scaffold = resolve_answers(
        &mut changes,
        &dest,
        EscapeLanguage::TypeScript,
        t.path(),
        "20260808000000",
        Some(&mut ask),
        "unused",
        (1, 2),
    )
    .unwrap()
    .expect("an escape answer writes a scaffold");

    assert!(scaffold.exists(), "{}", scaffold.display());
    assert!(
        scaffold.ends_with("transform.ts"),
        "the language decides the file: {}",
        scaffold.display()
    );

    let Some(Answer::Escape {
        language,
        file,
        scaffold_checksum,
    }) = changes[0].answer()
    else {
        panic!("expected an escape answer, got {:?}", changes[0].answer());
    };
    assert_eq!(*language, EscapeLanguage::TypeScript);
    assert_eq!(file, "transform.ts");
    assert_eq!(
        *scaffold_checksum,
        forgedb_migrations::checksum::compute(&std::fs::read(&scaffold).unwrap()),
        "the recorded hash is of the bytes ForgeDB just wrote"
    );
    assert_eq!(
        changes[0].answer(),
        changes[1].answer(),
        "every escape in one migration shares ONE file"
    );

    let m = Migration::with_id("20260808000000".into(), "x".into(), changes, 1, 2);
    assert!(hop_answer_status(t.path(), &m).is_err());

    std::fs::write(&scaffold, "export function transform(m, r) { return r }\n").unwrap();
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}

#[test]
fn the_scaffold_does_not_teach_the_todo_convention() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    for lang in [
        EscapeLanguage::Rust,
        EscapeLanguage::TypeScript,
        EscapeLanguage::Python,
    ] {
        let mut changes = vec![retype_views()];
        let mut ask = Scripted::new(["3"]);
        let scaffold = resolve_answers(
            &mut changes,
            &dest,
            lang,
            t.path(),
            &format!("2026080800{:04}", lang as u8),
            Some(&mut ask),
            "unused",
            (1, 2),
        )
        .unwrap()
        .expect("an escape answer writes a scaffold");
        let body = std::fs::read_to_string(&scaffold).unwrap();
        assert!(
            !body.contains("TODO"),
            "the {lang:?} scaffold still teaches TODO:\n{body}"
        );
        assert!(
            body.contains("Post"),
            "the scaffold names the model whose residue it is for:\n{body}"
        );
    }
}

#[test]
fn an_existing_authored_file_is_never_clobbered() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let dir = forgedb_migrations::migration_body_dir(t.path(), "20260808000000");
    std::fs::create_dir_all(&dir).unwrap();
    let authored = "export function transform(m, r) { return r } // mine\n";
    std::fs::write(dir.join("transform.ts"), authored).unwrap();

    let mut changes = vec![retype_views()];
    let mut ask = Scripted::new(["3"]);
    resolve_answers(
        &mut changes,
        &dest,
        EscapeLanguage::TypeScript,
        t.path(),
        "20260808000000",
        Some(&mut ask),
        "unused",
        (1, 2),
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.join("transform.ts")).unwrap(),
        authored,
        "an authored body is authoritative and is never rewritten"
    );
    let m = Migration::with_id("20260808000000".into(), "x".into(), changes, 1, 2);
    assert_eq!(
        hop_answer_status(t.path(), &m),
        Ok(()),
        "the recorded hash is of the bytes on disk, so an already-authored file \
         reads as authored rather than as an unedited scaffold"
    );
}

#[test]
fn scenario_22_the_escape_language_is_derived_never_declared() {
    let l = |targets: &[&str]| {
        language_for(&targets.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    };
    assert_eq!(l(&["typescript"]), EscapeLanguage::TypeScript, "node/bun sdk");
    assert_eq!(l(&["napi"]), EscapeLanguage::TypeScript, "node/bun runtime");
    assert_eq!(l(&["pyo3"]), EscapeLanguage::Python, "python runtime");
    assert_eq!(l(&["python-sdk"]), EscapeLanguage::Python);
    assert_eq!(l(&["rust"]), EscapeLanguage::Rust);
    assert_eq!(l(&["go", "go-sdk"]), EscapeLanguage::Rust);
    assert_eq!(l(&["pyo3", "typescript"]), EscapeLanguage::TypeScript);
    assert_eq!(l(&["typescript", "pyo3"]), EscapeLanguage::TypeScript);
    assert_eq!(l(&["rust", "pyo3"]), EscapeLanguage::Python);
    assert_eq!(l(&[]), EscapeLanguage::Rust, "no target implies no runtime");
}

use forgedb::commands::migrate::answers::resolve_rename_proposals;
use forgedb_migrations::{RenameProposal, SchemaDiffer, SimpleField, SimpleModel, SimpleSchema};

fn field(name: &str, ty: SimpleType) -> SimpleField {
    SimpleField {
        name: name.into(),
        ty,
        nullable: false,
        unique: false,
        indexed: false,
        index_type: "Hash".into(),
        constraints: vec![],
        depends_on: vec![],
    }
}

fn schema(fields: Vec<SimpleField>) -> SimpleSchema {
    SimpleSchema {
        models: vec![SimpleModel {
            name: "Post".into(),
            fields,
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    }
}

#[test]
fn scenario_10a_the_differ_proposes_and_still_emits_the_pair() {
    let old = schema(vec![field("id", SimpleType::Uuid), field("email", SimpleType::Str)]);
    let new = schema(vec![
        field("id", SimpleType::Uuid),
        field("username", SimpleType::Str),
    ]);
    let d = SchemaDiffer::diff(&old, &new);

    assert_eq!(
        d.rename_proposals,
        vec![RenameProposal::Field {
            model_name: "Post".into(),
            old_name: "email".into(),
            new_name: "username".into(),
        }]
    );
    let kinds: Vec<&str> = d
        .changes
        .iter()
        .map(|c| match c {
            SchemaChange::RemoveField { .. } => "remove",
            SchemaChange::AddField { .. } => "add",
            SchemaChange::RenameField { .. } => "rename",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["remove", "add"],
        "the diff itself must carry the drop+add, so DECLINING the proposal needs \
         nothing added: {:?}",
        d.changes
    );
}

#[test]
fn scenario_10b_declining_leaves_a_drop_and_an_add() {
    let old = schema(vec![field("id", SimpleType::Uuid), field("email", SimpleType::Str)]);
    let new = schema(vec![
        field("id", SimpleType::Uuid),
        field("username", SimpleType::Str),
    ]);
    let mut d = SchemaDiffer::diff(&old, &new);

    let mut ask = Scripted::new(["n"]);
    resolve_rename_proposals(&d.rename_proposals, &mut d.changes, Some(&mut ask)).unwrap();

    assert_eq!(d.changes.len(), 2, "{:?}", d.changes);
    assert!(
        d.changes
            .iter()
            .any(|c| matches!(c, SchemaChange::RemoveField { field_name, .. } if field_name == "email"))
    );
    let add = d
        .changes
        .iter()
        .find(|c| matches!(c, SchemaChange::AddField { .. }))
        .expect("the add survives");
    assert_eq!(
        add.hop_body_class(),
        forgedb_migrations::HopBodyClass::Authored,
        "a required add with no default is unprovable and gets its own question"
    );
    assert!(
        ask.is_exhausted(),
        "exactly one question was asked about the rename"
    );
}

#[test]
fn scenario_10c_accepting_replaces_the_pair_with_one_rename() {
    let old = schema(vec![field("id", SimpleType::Uuid), field("email", SimpleType::Str)]);
    let new = schema(vec![
        field("id", SimpleType::Uuid),
        field("username", SimpleType::Str),
    ]);
    let mut d = SchemaDiffer::diff(&old, &new);

    let mut ask = Scripted::new(["y"]);
    resolve_rename_proposals(&d.rename_proposals, &mut d.changes, Some(&mut ask)).unwrap();

    assert_eq!(
        d.changes,
        vec![SchemaChange::RenameField {
            model_name: "Post".into(),
            old_name: "email".into(),
            new_name: "username".into(),
        }],
        "accepting is a REPLACEMENT: the drop and the add must both be gone"
    );
}

#[test]
fn scenario_10d_a_proposal_with_nobody_to_ask_is_declined() {
    let old = schema(vec![field("id", SimpleType::Uuid), field("email", SimpleType::Str)]);
    let new = schema(vec![
        field("id", SimpleType::Uuid),
        field("username", SimpleType::Str),
    ]);
    let mut d = SchemaDiffer::diff(&old, &new);
    resolve_rename_proposals(&d.rename_proposals, &mut d.changes, None).unwrap();
    assert_eq!(d.changes.len(), 2, "{:?}", d.changes);
    assert!(!d.changes.iter().any(|c| matches!(c, SchemaChange::RenameField { .. })));
}

#[test]
fn a_model_rename_is_proposed_and_replaces_both_halves() {
    let one = |name: &str| SimpleSchema {
        models: vec![SimpleModel {
            name: name.into(),
            fields: vec![field("id", SimpleType::Uuid)],
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    };
    let mut d = SchemaDiffer::diff(&one("Post"), &one("Article"));
    assert_eq!(
        d.rename_proposals,
        vec![RenameProposal::Model {
            old_name: "Post".into(),
            new_name: "Article".into(),
        }]
    );
    let mut ask = Scripted::new(["y"]);
    resolve_rename_proposals(&d.rename_proposals, &mut d.changes, Some(&mut ask)).unwrap();
    assert_eq!(
        d.changes,
        vec![SchemaChange::RenameModel {
            old_name: "Post".into(),
            new_name: "Article".into(),
        }]
    );
}

use forgedb::commands::migrate::{Fill, lower_fill};

#[test]
fn decision_5_a_required_add_with_no_answer_lowers_to_nothing() {
    let constant = Answer::Constant {
        json: "\"x\"".into(),
    };
    let copy = Answer::CopyField {
        field: "title".into(),
    };
    let escape = Answer::Escape {
        language: EscapeLanguage::TypeScript,
        file: "transform.ts".into(),
        scaffold_checksum: "fnv1a64:0".into(),
    };

    assert_eq!(
        lower_fill(false, Some("\"pending\""), Some(&constant)),
        Some(Fill::Json("\"pending\"".into())),
        "a schema default is applied by BOTH routes; an answer is applied by the \
         transformer only, so the two are not interchangeable"
    );
    assert_eq!(
        lower_fill(false, None, Some(&constant)),
        Some(Fill::Json("\"x\"".into()))
    );
    assert_eq!(
        lower_fill(false, None, Some(&copy)),
        Some(Fill::Copy("title".into()))
    );
    assert_eq!(lower_fill(false, None, Some(&escape)), None);
    assert_eq!(lower_fill(true, None, None), Some(Fill::Json("null".into())));
    assert_eq!(
        lower_fill(false, None, None),
        None,
        "a required add with no default and no answer must emit NO op, so the key \
         is absent and the decode fails naming it. A type-zero here is what made \
         an unanswered hop write \"\" and exit 0."
    );
}

use forgedb::commands::migrate::escape::write_support_files;

const TYPED: &str = "enum Status { Draft, Published }\n\n\
                     struct Point {\n  x: u32\n  y: u32\n}\n\n\
                     Author {\n  id: +uuid\n  name: string\n}\n\n\
                     Post {\n  id: +uuid\n  title: string(24)\n  views: u32?\n  \
                     at: timestamp(us)\n  price: decimal\n  meta: json\n  \
                     state: Status\n  origin: Point\n  tags: [u32; 3]\n  \
                     author: *Author\n  editor: ?Author\n  raw: bytes(8)\n}\n";

#[test]
fn the_support_files_are_forgedbs_and_are_always_rewritten() {
    let t = TempDir::new().unwrap();
    let schema = parse(TYPED);
    let versions = [(1u32, &schema), (2u32, &schema)];

    let written =
        write_support_files(t.path(), "20260808000000", EscapeLanguage::TypeScript, &versions)
            .unwrap();
    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["host.ts", "v1.ts", "v2.ts"]);

    std::fs::write(&written[1], "// stale\n").unwrap();
    write_support_files(t.path(), "20260808000000", EscapeLanguage::TypeScript, &versions).unwrap();
    assert!(
        !std::fs::read_to_string(&written[1]).unwrap().contains("stale"),
        "ForgeDB's own modules are regenerated every time"
    );

    assert!(
        write_support_files(t.path(), "20260808000001", EscapeLanguage::Rust, &versions)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_typescript_module_types_every_wire_shape() {
    let schema = parse(TYPED);
    let (name, src) = forgedb_codegen::typescript_types(&schema, 2);
    assert_eq!(name, "v2.ts");

    for expected in [
        "export type Status = \"Draft\" | \"Published\";",
        "export interface Point {",
        "export interface Post {",
        "  title: string;",
        "  views: number | null;",
        "  at: string;",
        "  price: string;",
        "  meta: unknown;",
        "  state: Status;",
        "  origin: Point;",
        "  tags: number[];",
        "  author: string;",
        "  editor: string | null;",
        "  raw: number[];",
    ] {
        assert!(
            src.contains(expected),
            "the emitted v2.ts is missing {expected:?}:\n{src}"
        );
    }
    assert!(
        !src.contains(": any"),
        "`any` in a module whose whole purpose is to type the author's transform \
         is a type that checks nothing:\n{src}"
    );
    assert!(src.contains("DO NOT EDIT"), "it is ForgeDB's file:\n{src}");
}

#[test]
fn the_python_module_types_every_wire_shape() {
    let schema = parse(TYPED);
    let (name, src) = forgedb_codegen::python_types(&schema, 1);
    assert_eq!(name, "v1.py");
    for expected in [
        "Status = Literal[\"Draft\", \"Published\"]",
        "class Point(TypedDict):",
        "class Post(TypedDict):",
        "    title: str",
        "    views: Optional[int]",
        "    at: str",
        "    price: str",
        "    meta: Any",
        "    state: Status",
        "    origin: Point",
        "    tags: List[int]",
        "    author: str",
        "    editor: Optional[str]",
        "    raw: List[int]",
    ] {
        assert!(
            src.contains(expected),
            "the emitted v1.py is missing {expected:?}:\n{src}"
        );
    }
}

#[test]
fn a_collection_relation_is_absent_from_both_modules() {
    let schema = parse(
        "Author {\n  id: +uuid\n  name: string\n  posts: [Post]\n}\n\n\
         Post {\n  id: +uuid\n  author: *Author\n}\n",
    );
    let (_, ts) = forgedb_codegen::typescript_types(&schema, 1);
    let (_, py) = forgedb_codegen::python_types(&schema, 1);
    assert!(!ts.contains("posts"), "{ts}");
    assert!(!py.contains("posts"), "{py}");
    assert!(!ts.contains("never"), "and no placeholder stands in for it:\n{ts}");
}
