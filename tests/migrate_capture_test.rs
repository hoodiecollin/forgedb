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
