use forgedb_migrations::{
    Answer, EscapeLanguage, Migration, SchemaChange, Unanswered, authored_body_path, checksum,
    hop_answer_status, migration_body_dir,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

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

#[test]
fn scenario_12b_an_authored_file_that_still_says_todo_builds() {
    let t = TempDir::new().unwrap();
    let scaffold = "// TODO: re-encode Post.views\nexport function transformPost(p) { return p }\n";
    let sum = write_escape(t.path(), "20260808120000", EscapeLanguage::TypeScript, scaffold);

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

#[test]
fn a_legacy_record_builds_from_its_transform_rs() {
    let t = TempDir::new().unwrap();
    let mut m = migration(vec![retype(None)]);
    m.record_version = 0;

    let Err(problems) = hop_answer_status(t.path(), &m) else {
        panic!("a legacy record with no body must refuse");
    };
    assert!(
        matches!(problems[0], Unanswered::LegacyBodyMissing { .. }),
        "a legacy record must not be told it is missing an ANSWER, which it could \
         never have carried: {problems:?}"
    );

    let path = authored_body_path(t.path(), &m.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "pub fn authored_transform() {}\n").unwrap();
    assert_eq!(hop_answer_status(t.path(), &m), Ok(()));
}
