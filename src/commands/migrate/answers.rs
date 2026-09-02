use crate::ask::{Choice, Prompt};
use crate::{Result, error::CliError};
use forgedb_migrations::{Answer, EscapeLanguage, HopBodyClass, RenameProposal, SchemaChange};
use std::path::Path;

struct Question<'a> {
    change_index: usize,
    subject: String,
    description: String,
    wanted: &'static str,
    field: Option<&'a forgedb_parser::Field>,
    copy_candidates: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_answers(
    changes: &mut [SchemaChange],
    dest_schema: &forgedb_parser::Schema,
    escape: EscapeLanguage,
    migrations_dir: &Path,
    migration_id: &str,
    ask: Option<&mut (dyn Prompt + '_)>,
    non_interactive_reason: &str,
    versions: (u32, u32),
) -> Result<Option<std::path::PathBuf>> {
    let questions = collect_questions(changes, dest_schema);
    if questions.is_empty() {
        return Ok(None);
    }

    let Some(ask) = ask else {
        let q = &questions[0];
        return Err(CliError::Migration(format!(
            "{} needs an answer and this session cannot ask for one ({non_interactive_reason}).\n\
             \n  {}\n  ForgeDB cannot derive {} from the schema diff.\n\n\
             Run `forgedb migrate create` in a terminal to answer it, or give the field a \
             `@default` in the schema so there is nothing to ask.\n\
             No migration was written.",
            q.subject, q.description, q.wanted,
        )));
    };

    let mut escapes: Vec<usize> = Vec::new();
    for q in &questions {
        let answer = ask_one(ask, q, dest_schema, escape)?;
        match answer {
            Some(a) => {
                if !changes[q.change_index].set_answer(a) {
                    return Err(CliError::Migration(format!(
                        "internal error: {} was classified as needing an answer but \
                         carries no slot for one",
                        q.subject
                    )));
                }
            }
            None => escapes.push(q.change_index),
        }
    }

    if escapes.is_empty() {
        return Ok(None);
    }

    let (path, scaffold_checksum) = super::escape::write_scaffold(
        migrations_dir,
        migration_id,
        escape,
        changes,
        dest_schema,
        versions,
    )?;
    for i in escapes {
        changes[i].set_answer(Answer::Escape {
            language: escape,
            file: escape.transform_file(),
            scaffold_checksum: scaffold_checksum.clone(),
        });
    }
    Ok(Some(path))
}

fn collect_questions<'a>(
    changes: &[SchemaChange],
    dest_schema: &'a forgedb_parser::Schema,
) -> Vec<Question<'a>> {
    let mut out = Vec::new();
    for (i, change) in changes.iter().enumerate() {
        if change.hop_body_class() != HopBodyClass::Authored || change.answer().is_some() {
            continue;
        }
        let (model, field_name, wanted) = match change {
            SchemaChange::AddField {
                model_name,
                field_name,
                ..
            } => (
                model_name.as_str(),
                Some(field_name.as_str()),
                "a value for the rows that already exist",
            ),
            SchemaChange::ChangeFieldType {
                model_name,
                field_name,
                ..
            } => (
                model_name.as_str(),
                Some(field_name.as_str()),
                "how to re-encode the values already stored",
            ),
            SchemaChange::ChangeFieldNullability {
                model_name,
                field_name,
                ..
            } => (
                model_name.as_str(),
                Some(field_name.as_str()),
                "a value for the rows whose field is currently null",
            ),
            other => (other.target_model(), None, "how to remap the stored values"),
        };
        let field = field_name.and_then(|f| {
            dest_schema
                .models
                .iter()
                .find(|m| m.name == model)
                .and_then(|m| m.fields.iter().find(|x| x.name == f))
        });
        out.push(Question {
            change_index: i,
            subject: match field_name {
                Some(f) => format!("{model}.{f}"),
                None => model.to_string(),
            },
            description: change.description(),
            wanted,
            field,
            copy_candidates: copy_candidates(dest_schema, model, field),
        });
    }
    out
}

fn copy_candidates(
    dest_schema: &forgedb_parser::Schema,
    model: &str,
    field: Option<&forgedb_parser::Field>,
) -> Vec<String> {
    let Some(field) = field else {
        return Vec::new();
    };
    let want = super::to_simple_type(&field.field_type);
    dest_schema
        .models
        .iter()
        .find(|m| m.name == model)
        .map(|m| {
            m.fields
                .iter()
                .filter(|f| f.name != field.name)
                .filter(|f| super::to_simple_type(&f.field_type) == want)
                .map(|f| f.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn ask_one(
    ask: &mut dyn Prompt,
    q: &Question<'_>,
    dest_schema: &forgedb_parser::Schema,
    escape: EscapeLanguage,
) -> Result<Option<Answer>> {
    let mut options = vec!["a constant value".to_string()];
    let mut kinds = vec!["constant"];
    if !q.copy_candidates.is_empty() {
        options.push(format!(
            "copy another field           {}",
            q.copy_candidates.join(", ")
        ));
        kinds.push("copy");
    }
    options.push(format!(
        "leave it — I'll write the transform in {}   (advanced)",
        language_label(escape)
    ));
    kinds.push("escape");

    let (options, kinds) = if q.field.is_none() {
        (
            vec![options.pop().expect("the escape row is always last")],
            vec!["escape"],
        )
    } else {
        (options, kinds)
    };

    let question = format!(
        "{}\n{} — ForgeDB cannot derive {}.\nWhat should existing rows get?",
        q.description, q.subject, q.wanted
    );

    loop {
        let choice = ask.select(&question, &options, None)?;
        let Choice::Index(i) = choice else {
            unreachable!("select without a free-text hint returns an index");
        };
        match kinds[i] {
            "escape" => return Ok(None),
            "copy" => {
                let picked = ask
                    .select(
                        &format!("Which field should {} copy?", q.subject),
                        &q.copy_candidates,
                        None,
                    )
                    ?;
                let Choice::Index(j) = picked else {
                    unreachable!("select without a free-text hint returns an index")
                };
                return Ok(Some(Answer::CopyField {
                    field: q.copy_candidates[j].clone(),
                }));
            }
            _ => {
                let typed = ask
                    .select(
                        &format!("What constant should {} get?", q.subject),
                        &[],
                        Some("the value"),
                    )
                    ?;
                let Choice::Free(text) = typed else {
                    unreachable!("a menu with no options and a free hint returns free text")
                };
                let field = q.field.expect("a constant is only offered for a field");
                match forgedb_codegen::fill_from_param(
                    dest_schema,
                    field,
                    &forgedb_parser::ConstraintParam::String(text.clone()),
                ) {
                    Some(fill) => {
                        return Ok(Some(Answer::Constant {
                            json: fill.json_literal(),
                        }));
                    }
                    None => {
                        crate::ui::warning(&format!(
                            "{text:?} is not a value {} can hold. Try again, or choose \
                             the transform option.",
                            q.subject
                        ));
                    }
                }
            }
        }
    }
}

fn language_label(l: EscapeLanguage) -> &'static str {
    match l {
        EscapeLanguage::Rust => "Rust",
        EscapeLanguage::TypeScript => "TypeScript",
        EscapeLanguage::Python => "Python",
    }
}

pub fn resolve_rename_proposals(
    proposals: &[RenameProposal],
    changes: &mut Vec<SchemaChange>,
    mut ask: Option<&mut (dyn Prompt + '_)>,
) -> Result<()> {
    for proposal in proposals {
        let (question, accepted_change, drops_and_adds): (String, SchemaChange, Vec<SchemaChange>) =
            match proposal {
                RenameProposal::Field {
                    model_name,
                    old_name,
                    new_name,
                } => (
                    format!(
                        "{model_name}.{old_name} is gone and {model_name}.{new_name} is new, \
                         with the same type.\nIs that a rename? (yes keeps the stored values; \
                         no drops the column and starts the new one empty)"
                    ),
                    SchemaChange::RenameField {
                        model_name: model_name.clone(),
                        old_name: old_name.clone(),
                        new_name: new_name.clone(),
                    },
                    vec![
                        SchemaChange::RemoveField {
                            model_name: model_name.clone(),
                            field_name: old_name.clone(),
                        },
                        SchemaChange::AddField {
                            model_name: model_name.clone(),
                            field_name: new_name.clone(),
                            field_type: forgedb_migrations::SimpleType::Opaque(String::new()),
                            nullable: false,
                            default_json: None,
                            answer: None,
                        },
                    ],
                ),
                RenameProposal::Model { old_name, new_name } => (
                    format!(
                        "Model {old_name} is gone and {new_name} is new, with the same \
                         shape.\nIs that a rename? (yes carries every row across; no drops \
                         {old_name}'s rows and starts {new_name} empty)"
                    ),
                    SchemaChange::RenameModel {
                        old_name: old_name.clone(),
                        new_name: new_name.clone(),
                    },
                    vec![
                        SchemaChange::RemoveModel {
                            model_name: old_name.clone(),
                        },
                        SchemaChange::AddModel {
                            model_name: new_name.clone(),
                        },
                    ],
                ),
            };

        let accepted = match ask.as_deref_mut() {
            Some(a) => a.confirm(&question)?,
            None => false,
        };
        if !accepted {
            continue;
        }

        changes.retain(|c| !same_subject(c, &drops_and_adds));
        changes.push(accepted_change);
    }
    Ok(())
}

fn same_subject(c: &SchemaChange, pair: &[SchemaChange]) -> bool {
    fn key(c: &SchemaChange) -> Option<(&str, Option<&str>)> {
        match c {
            SchemaChange::RemoveField {
                model_name,
                field_name,
            } => Some((model_name, Some(field_name))),
            SchemaChange::AddField {
                model_name,
                field_name,
                ..
            } => Some((model_name, Some(field_name))),
            SchemaChange::RemoveModel { model_name } | SchemaChange::AddModel { model_name } => {
                Some((model_name, None))
            }
            _ => None,
        }
    }
    let kind = |c: &SchemaChange| std::mem::discriminant(c);
    pair.iter()
        .any(|p| kind(p) == kind(c) && key(p).is_some() && key(p) == key(c))
}
