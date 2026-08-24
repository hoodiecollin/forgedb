//! The interactive half of `migrate create` (#374 direction A), driven by a
//! **scripted operator**.
//!
//! # Why this file exists at all
//!
//! Every other `migrate` test in this repo drives the `forgedb` binary as a
//! subprocess with piped stdio. `prompt::askable` correctly refuses to ask in
//! that session, so those tests can only ever walk the NON-interactive branch —
//! the branch a real operator walks would otherwise be covered by nothing.
//! `prompt::Scripted` is what makes it testable, and this is why the `Ask`
//! trait exists rather than `resolve_answers` calling `read_line` directly.

use forgedb::commands::migrate::answers::resolve_answers;
use forgedb::commands::migrate::escape::language_for;
use forgedb::prompt::Scripted;
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

/// Scenario 5 — the answer is recorded as **data**, and the record's checksum
/// covers it.
#[test]
fn scenario_5_a_constant_is_recorded_as_data() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let mut changes = vec![add_slug()];
    // "1" = a constant value, then the value itself.
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

    // And it survives into a record whose checksum covers it.
    let m = Migration::with_id("20260808000000".into(), "x".into(), changes, 1, 2);
    assert_eq!(m.record_version, 1);
    assert!(m.verify_checksum());
    assert!(m.unanswered().is_empty());
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}

/// A copy is offered only when the model HAS a field of the same type, and the
/// recorded answer names it.
#[test]
fn a_copy_answer_names_a_field_of_the_same_type() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let mut changes = vec![add_slug()];
    // "2" = copy another field, then pick from the candidate list. `title` and
    // `views` are both `string`, and `title` sorts first in declaration order.
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

/// A model with no field of the matching type is not offered the copy option at
/// all — an option that cannot work is an invitation to an answer the build
/// would then refuse.
#[test]
fn the_copy_option_is_absent_when_nothing_could_be_copied() {
    let t = TempDir::new().unwrap();
    let dest = parse("Post {\n  id: +uuid\n  views: u32\n  slug: string\n}\n");
    let mut changes = vec![add_slug()];
    // "2" would be the escape row here, not a copy. Answer "1" (constant).
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
    // With a copy row present the menu would have three entries; here it has two.
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

/// The escape hatch writes ONE scaffold for the whole migration, hashes it into
/// every escaping change's answer, and that hash is what `migrate build`
/// compares against.
#[test]
fn the_escape_hatch_records_the_scaffolds_own_hash() {
    let t = TempDir::new().unwrap();
    let dest = parse(DEST);
    let mut changes = vec![add_slug(), retype_views()];
    // Escape for both. `slug` offers constant / copy / escape (3 rows);
    // `views` the same.
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

    // Unedited: the build refuses.
    let m = Migration::with_id("20260808000000".into(), "x".into(), changes, 1, 2);
    assert!(hop_answer_status(t.path(), &m).is_err());

    // Authored: the build proceeds.
    std::fs::write(&scaffold, "export function transform(m, r) { return r }\n").unwrap();
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}

/// The scaffold does **not** teach the `TODO` convention.
///
/// The old text said "fill in every TODO, then this migration is ready to
/// build". That sentence taught a convention the build-time refusal must not
/// use: a `TODO` grep is satisfied by deleting a comment and refuses a
/// genuinely authored file that happens to contain the word.
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

/// An existing authored file is never clobbered, and the hash returned is of
/// what is ON DISK — so a re-run of `migrate create` cannot un-answer a
/// migration by overwriting the author's work with a fresh scaffold.
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

/// Scenario 22 — the escape language is **derived** from `[generate].targets`,
/// and there is no config key that can disagree with it.
#[test]
fn scenario_22_the_escape_language_is_derived_never_declared() {
    let l = |targets: &[&str]| {
        language_for(&targets.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    };
    // The INTERNAL names `targets::resolve` produces.
    assert_eq!(l(&["typescript"]), EscapeLanguage::TypeScript, "node/bun sdk");
    assert_eq!(l(&["napi"]), EscapeLanguage::TypeScript, "node/bun runtime");
    assert_eq!(l(&["pyo3"]), EscapeLanguage::Python, "python runtime");
    assert_eq!(l(&["python-sdk"]), EscapeLanguage::Python);
    assert_eq!(l(&["rust"]), EscapeLanguage::Rust);
    // Go is COMPILED, so "run the author's own runtime out of process" would
    // mean invoking a toolchain and linking generated packages — materially
    // more than the line-oriented host loop. It falls back to Rust.
    assert_eq!(l(&["go", "go-sdk"]), EscapeLanguage::Rust);
    // Precedence is fixed rather than taken from the user's ordering, so a
    // project declaring several (or `all`, a set with no order) resolves the
    // same way every time.
    assert_eq!(l(&["pyo3", "typescript"]), EscapeLanguage::TypeScript);
    assert_eq!(l(&["typescript", "pyo3"]), EscapeLanguage::TypeScript);
    assert_eq!(l(&["rust", "pyo3"]), EscapeLanguage::Python);
    assert_eq!(l(&[]), EscapeLanguage::Rust, "no target implies no runtime");
}

// ---------------------------------------------------------------------------
// Scenario 10 — a rename is proposed, never assumed
// ---------------------------------------------------------------------------

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

/// The differ **proposes** and emits the drop+add; it decides nothing.
///
/// A guess that is right most of the time is the worst shape available here:
/// a rename carries every stored value across, a drop+add empties the column,
/// and the wrong half succeeds silently.
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

/// Answering "no, these are unrelated" leaves the drop+add — and the add is
/// itself unprovable, so it gets its own question.
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

/// Answering "yes" replaces the pair with exactly one `RenameField`.
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

/// Non-interactively the proposal is **declined**.
///
/// A drop+add is what the schema literally says; inferring otherwise with
/// nobody to check is exactly the guess #374 removes. It is also the safe
/// direction to be wrong in — a spurious drop+add is visible in the report and
/// costs a re-run, while a spurious rename is silent.
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

/// A model rename is the same shape, and accepting it must clear BOTH the
/// `RemoveModel` and the `AddModel`.
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

// ---------------------------------------------------------------------------
// Decision 5 — a required add with no answer emits NOTHING
// ---------------------------------------------------------------------------

use forgedb::commands::migrate::{Fill, lower_fill};

/// `lower_fill` is the one place a field's value is decided, and its precedence
/// is stated rather than emergent.
///
/// The last row is decision 5. A required add with no default and no answer
/// contributes **no op**, so the key is ABSENT from the row and the destination
/// decode fails with `missing field`, naming it. Returning a type-zero here is
/// what made an unanswered hop write `""` and exit 0 — a successful exit is the
/// defect's whole signature, which is why the end-to-end half of this
/// (`tests/migrate_escape_test.rs` scenario 15) has to RUN the generated hop.
///
/// Mutation-verified: making the final arm `Some(Fill::Json("\"\"".into()))`
/// turns this test red, and scenario 15 red with it.
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

    // 1. The schema's `@default` wins, whatever else is present.
    assert_eq!(
        lower_fill(false, Some("\"pending\""), Some(&constant)),
        Some(Fill::Json("\"pending\"".into())),
        "a schema default is applied by BOTH routes; an answer is applied by the \
         transformer only, so the two are not interchangeable"
    );
    // 2. Then the operator's answer.
    assert_eq!(
        lower_fill(false, None, Some(&constant)),
        Some(Fill::Json("\"x\"".into()))
    );
    assert_eq!(
        lower_fill(false, None, Some(&copy)),
        Some(Fill::Copy("title".into()))
    );
    // 3. An escape's value comes from the author's transform, which runs AFTER
    //    these structural ops — so there is nothing to emit here.
    assert_eq!(lower_fill(false, None, Some(&escape)), None);
    // 4. `null` for a nullable add: the only value a nullable field can be given
    //    without asking.
    assert_eq!(lower_fill(true, None, None), Some(Fill::Json("null".into())));
    // 5. DECISION 5. Nothing.
    assert_eq!(
        lower_fill(false, None, None),
        None,
        "a required add with no default and no answer must emit NO op, so the key \
         is absent and the decode fails naming it. A type-zero here is what made \
         an unanswered hop write \"\" and exit 0."
    );
}
