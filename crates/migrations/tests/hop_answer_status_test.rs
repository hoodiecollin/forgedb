//! `migrate build` refuses a hop that has no answer (#374 step 6, gate 1
//! decision 4) — and the refusal is a **hash comparison, not a `TODO` grep**.
//!
//! Both halves of that matter and they fail in opposite directions:
//!
//! * a `TODO` grep is satisfied by **deleting a comment**, so an author who
//!   tidied the scaffold without answering anything would build;
//! * a `TODO` grep also **refuses a genuinely authored file** that happens to
//!   still contain the word in a comment of its own.
//!
//! The hash is recorded once, at create, and never recomputed at build: a
//! scaffold regenerated at build time reads as equivalent and is not — any
//! improvement to the scaffold text would then make every previously-authored
//! file compare unequal to a scaffold that was never written.

use forgedb_migrations::{
    Answer, EscapeLanguage, Migration, SchemaChange, Unanswered, authored_body_path, checksum,
    hop_answer_status, migration_body_dir,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A `ChangeFieldType` — unprovable, so it needs an answer.
fn retype(answer: Option<Answer>) -> SchemaChange {
    SchemaChange::ChangeFieldType {
        model_name: "Post".into(),
        field_name: "views".into(),
        old_type: "u32".parse().unwrap(),
        new_type: "string".parse().unwrap(),
        answer,
    }
}

fn migration(changes: Vec<SchemaChange>) -> Migration {
    Migration::with_id("20260808120000".into(), "hop".into(), changes, 1, 2)
}

/// Write `bytes` as the migration's escape transform and return its checksum.
fn write_escape(dir: &Path, id: &str, lang: EscapeLanguage, bytes: &str) -> String {
    let body = migration_body_dir(dir, id);
    fs::create_dir_all(&body).unwrap();
    fs::write(body.join(lang.transform_file()), bytes).unwrap();
    checksum::compute(bytes.as_bytes())
}

#[test]
fn a_fully_provable_hop_needs_nothing() {
    let t = TempDir::new().unwrap();
    let m = migration(vec![SchemaChange::RemoveField {
        model_name: "Post".into(),
        field_name: "old".into(),
    }]);
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}

/// Scenario 11 at the predicate: an unprovable change with no answer refuses,
/// and the refusal names the change in the same words `migrate create` printed.
#[test]
fn scenario_11_an_unanswered_change_refuses_and_names_itself() {
    let t = TempDir::new().unwrap();
    let m = migration(vec![retype(None)]);
    let Err(problems) = hop_answer_status(t.path(), &m) else {
        panic!("an unanswered hop must not be buildable");
    };
    assert_eq!(problems.len(), 1);
    let Unanswered::NoAnswer { change } = &problems[0] else {
        panic!("expected NoAnswer, got {problems:?}");
    };
    assert!(
        change.contains("Post") && change.contains("views"),
        "the refusal must name the change: {change}"
    );
}

/// `Constant` and `CopyField` are decided from the record alone — nothing on
/// disk is consulted, and there is nothing on disk here.
#[test]
fn an_in_record_answer_consults_no_file() {
    let t = TempDir::new().unwrap();
    for answer in [
        Answer::Constant {
            json: "\"0\"".into(),
        },
        Answer::CopyField {
            field: "title".into(),
        },
    ] {
        let m = migration(vec![retype(Some(answer))]);
        assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
    }
}

#[test]
fn an_escape_whose_file_is_missing_refuses() {
    let t = TempDir::new().unwrap();
    let m = migration(vec![retype(Some(Answer::Escape {
        language: EscapeLanguage::TypeScript,
        file: "transform.ts".into(),
        scaffold_checksum: "fnv1a64:0000000000000000".into(),
    }))]);
    let Err(problems) = hop_answer_status(t.path(), &m) else {
        panic!("a missing escape file must refuse");
    };
    assert!(
        matches!(problems[0], Unanswered::EscapeFileMissing { .. }),
        "{problems:?}"
    );
}

/// Scenario 12, first half — the file is byte-identical to the scaffold, so
/// nothing was authored. This is the case a `TODO` grep also catches.
#[test]
fn scenario_12a_an_unedited_scaffold_refuses() {
    let t = TempDir::new().unwrap();
    let scaffold = "// TODO: re-encode Post.views\nexport function transformPost(p) { return p }\n";
    let sum = write_escape(t.path(), "20260808120000", EscapeLanguage::TypeScript, scaffold);
    let m = migration(vec![retype(Some(Answer::Escape {
        language: EscapeLanguage::TypeScript,
        file: "transform.ts".into(),
        scaffold_checksum: sum,
    }))]);
    let Err(problems) = hop_answer_status(t.path(), &m) else {
        panic!("an unedited scaffold must refuse");
    };
    assert!(
        matches!(problems[0], Unanswered::EscapeFileUnedited { .. }),
        "{problems:?}"
    );
}

/// Scenario 12, second half — a genuinely authored file that still contains the
/// word `TODO` in a comment **builds**. This is the case a `TODO` grep gets
/// WRONG, and it is the reason the check is a hash.
#[test]
fn scenario_12b_an_authored_file_that_still_says_todo_builds() {
    let t = TempDir::new().unwrap();
    let scaffold = "// TODO: re-encode Post.views\nexport function transformPost(p) { return p }\n";
    let sum = write_escape(t.path(), "20260808120000", EscapeLanguage::TypeScript, scaffold);

    // The author writes a real transform and leaves a TODO of their own.
    let authored = "// TODO: revisit this once analytics lands\n\
                    export function transformPost(p) { return { ...p, views: String(p.views) } }\n";
    write_escape(t.path(), "20260808120000", EscapeLanguage::TypeScript, authored);

    let m = migration(vec![retype(Some(Answer::Escape {
        language: EscapeLanguage::TypeScript,
        file: "transform.ts".into(),
        scaffold_checksum: sum,
    }))]);
    assert_eq!(
        hop_answer_status(t.path(), &m),
        Ok(()),
        "a `TODO` in an AUTHORED file must not refuse the build — that is exactly \
         what a TODO grep gets wrong, and why this is a hash comparison"
    );
}

/// Deleting the scaffold's `// TODO:` lines and nothing else **passes** this
/// check, and that is the honest limit.
///
/// Hash equality proves *untouched*; it cannot prove *answered*. What covers
/// this case is the other half of the same pair — with the defensive type-zero
/// gone, the hop's rows reach the destination decode without the required key
/// and fail NAMING the field. Asserted end-to-end by
/// `tests/migrate_answers_test.rs` scenario 13.
#[test]
fn deleting_the_todo_lines_passes_and_that_limit_is_deliberate() {
    let t = TempDir::new().unwrap();
    let scaffold = "// TODO: re-encode Post.views\nexport function transformPost(p) { return p }\n";
    let sum = write_escape(t.path(), "20260808120000", EscapeLanguage::TypeScript, scaffold);
    write_escape(
        t.path(),
        "20260808120000",
        EscapeLanguage::TypeScript,
        "export function transformPost(p) { return p }\n",
    );
    let m = migration(vec![retype(Some(Answer::Escape {
        language: EscapeLanguage::TypeScript,
        file: "transform.ts".into(),
        scaffold_checksum: sum,
    }))]);
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}

/// Every unanswered change is reported, not just the first.
///
/// `migrate build` is not the interactive surface — the operator is fixing a
/// committed lineage, and one refusal per rebuild is a loop. (`migrate create`
/// is the one that stops at the FIRST, because there the operator is answering
/// them one at a time.)
#[test]
fn the_build_refusal_reports_every_unanswered_change() {
    let t = TempDir::new().unwrap();
    let m = migration(vec![
        retype(None),
        SchemaChange::AddField {
            model_name: "Post".into(),
            field_name: "slug".into(),
            field_type: "string".parse().unwrap(),
            nullable: false,
            default_json: None,
            answer: None,
        },
    ]);
    let Err(problems) = hop_answer_status(t.path(), &m) else {
        panic!("must refuse");
    };
    assert_eq!(problems.len(), 2, "{problems:?}");
}

// ---------------------------------------------------------------------------
// The legacy arm — written first, not last (#374 gotcha 15)
// ---------------------------------------------------------------------------

/// A pre-#374 record with authored residue builds from its `transform.rs`, with
/// no answer to check it against.
///
/// Every lineage already committed is in this state. Without this arm, landing
/// #374 refuses all of them at build time with a message about an answer that
/// could never have been recorded.
#[test]
fn a_legacy_record_builds_from_its_transform_rs() {
    let t = TempDir::new().unwrap();
    let mut m = migration(vec![retype(None)]);
    m.record_version = 0;

    // Without the body: refused, but for the LEGACY reason.
    let Err(problems) = hop_answer_status(t.path(), &m) else {
        panic!("a legacy record with no body must refuse");
    };
    assert!(
        matches!(problems[0], Unanswered::LegacyBodyMissing { .. }),
        "a legacy record must not be told it is missing an ANSWER, which it could \
         never have carried: {problems:?}"
    );

    // With the body: builds, even though no change carries an answer.
    let path = authored_body_path(t.path(), &m.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "pub fn authored_transform() {}\n").unwrap();
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}
