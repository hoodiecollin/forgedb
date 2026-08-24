//! Capturing the answer at `migrate create` time (#374 direction A).
//!
//! `migrate create` already knows *exactly* which changes it cannot prove, at
//! the moment the author has the change in their head. That is when to ask —
//! not at `migrate build`, weeks later, in a scaffold of TODOs.
//!
//! # What is recorded, and what is not
//!
//! The answer is recorded as **data in the migration record**, never as code.
//! `migrate build` LOWERS it into the emitted hop (`super::lower_fill`); it is
//! never carried into generated code as data and never matched on at run time.
//!
//! # Two ordering constraints that are not optimisations
//!
//! 1. **Every answer is resolved before the first write.** `create` writes the
//!    record, then the snapshot, then the versioned schema. Prompting in the
//!    middle would add a window in which the operator answers, hits Ctrl-C, and
//!    leaves a partial lineage behind. Resolving first means an abandoned
//!    prompt leaves the tree exactly as it was.
//! 2. **The migration id is allocated before the record exists.** An
//!    `Answer::Escape` scaffold lives at `migrations/<id>/transform.<ext>` and
//!    its hash has to be inside the record before the record's own checksum is
//!    computed — otherwise the checksum does not cover the answer, and the
//!    answer can be edited afterwards with the record still reading as intact.

use crate::ask::{Choice, Prompt};
use crate::{Result, error::CliError};
use forgedb_migrations::{Answer, EscapeLanguage, HopBodyClass, RenameProposal, SchemaChange};
use std::path::Path;

/// A change the differ could not prove, with everything needed to ask about it.
struct Question<'a> {
    change_index: usize,
    /// `Model.field`, the name the operator sees in every message.
    subject: String,
    /// The change's own description — the same sentence `migrate create` printed.
    description: String,
    /// What ForgeDB wanted and could not derive.
    wanted: &'static str,
    /// The destination field, when there is one, for type-checking a constant
    /// and for listing copy candidates.
    field: Option<&'a forgedb_parser::Field>,
    /// Fields of the same model with an identical resolved type.
    copy_candidates: Vec<String>,
}

/// Fill each unprovable change's `answer` in place (#374 step 8).
///
/// `ask == None` is the **non-interactive contract**: the FIRST change needing
/// an answer returns an error naming the change and what it wanted. Not a
/// collected batch — a CI run gets a specific failure at the first instance,
/// and a batch of ten reads as ten problems when it is one schema edit.
///
/// (The build-time refusal is the opposite and deliberately so: there the
/// operator is fixing a committed lineage, and one refusal per rebuild is a
/// loop.)
///
/// Returns the [`EscapeLanguage`] scaffold that was written, if any.
///
/// The argument list is long because every one of them is a distinct fact this
/// function needs and none is derivable from the others; bundling them into a
/// struct would move the same eight values one line up, at one call site.
#[allow(clippy::too_many_arguments)]
pub fn resolve_answers(
    changes: &mut [SchemaChange],
    dest_schema: &forgedb_parser::Schema,
    escape: EscapeLanguage,
    migrations_dir: &Path,
    migration_id: &str,
    ask: Option<&mut (dyn Prompt + '_)>,
    non_interactive_reason: &str,
    // The hop's `(from, to)` schema serials — the two typed modules an escape
    // transform reads from and writes to.
    versions: (u32, u32),
) -> Result<Option<std::path::PathBuf>> {
    let questions = collect_questions(changes, dest_schema);
    if questions.is_empty() {
        return Ok(None);
    }

    let Some(ask) = ask else {
        // The FIRST one, named. The rest are deliberately not mentioned.
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

    // Every escape in one migration shares ONE authored file, so it is written
    // once — after every prompt, so an abandoned session leaves nothing behind.
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

/// Every change that needs an answer, in record order.
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
            // An enum/struct definition change that is `Authored` (a dropped or
            // renamed variant, a retyped struct member). It carries no slot for
            // an answer, so the escape hatch is its only route — and
            // `set_answer` would refuse it. Named here so the operator is told
            // that rather than left to discover it at build time.
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

/// Fields of `model` with the same resolved type as `field` — the only ones a
/// `CopyField` answer can name without the hop failing its destination decode.
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

/// Ask one question. `Ok(None)` means "the operator chose the escape hatch",
/// which the caller resolves once, for all of them, after every prompt.
fn ask_one(
    ask: &mut dyn Prompt,
    q: &Question<'_>,
    dest_schema: &forgedb_parser::Schema,
    escape: EscapeLanguage,
) -> Result<Option<Answer>> {
    // The menu is built from what is actually possible for THIS change, so an
    // option that cannot work is never offered. A "copy another field" row on a
    // model with no field of the same type is an invitation to an answer the
    // build would then refuse.
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

    // A change with no field (an enum/struct remap) cannot take a constant or a
    // copy; the escape hatch is the whole menu.
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
            // The menu offers no free-text escape, so `Ask` cannot return one.
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
                // The SAME conversion `@default` goes through — one definition,
                // so an operator's answer and a schema's default cannot be
                // encoded differently.
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
                        // Refused rather than substituted. The loop re-asks
                        // instead of erroring: the operator is right here, and
                        // a typo should cost a retype rather than the run.
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

/// Decide each proposed rename (#374 decision 10) — the differ's second half.
///
/// # Why the differ stopped deciding
///
/// One field dropped and one added of the same type is *usually* a rename, and
/// the differ used to say so outright. It is a guess, and the two readings
/// produce **opposite data**: a rename carries every stored value across, a
/// drop+add empties the column. A guess that is right most of the time is the
/// worst shape available here, because the wrong half succeeds silently — the
/// operator sees "Rename field 'Post.email' -> 'Post.username'" in a report they
/// have no reason to doubt, and finds out when the column is empty.
///
/// So: accepted, the proposal REPLACES the drop+add pair; declined, the pair is
/// already correct and nothing is added. Non-interactively the proposal is
/// **declined**, because a drop+add is what the schema literally says and
/// inferring otherwise is exactly the guess this removes.
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
                            // Matched by model+field below, so the remaining
                            // fields do not participate.
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
            // Declined. A drop+add is what the schema literally says.
            None => false,
        };
        if !accepted {
            continue;
        }

        // Replace the pair. Matched on identity (model + field / model name)
        // rather than on full equality, because the `AddField` in the diff
        // carries a real type and a resolved default that the placeholder above
        // deliberately does not.
        changes.retain(|c| !same_subject(c, &drops_and_adds));
        changes.push(accepted_change);
    }
    Ok(())
}

/// Does `c` name the same subject as one of `pair`?
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
    // Kinds must match too: a `RemoveField` must not cancel an `AddField` of
    // the same name in a model that has both (which cannot happen today, but a
    // key-only match would silently do the wrong thing if it ever could).
    let kind = |c: &SchemaChange| std::mem::discriminant(c);
    pair.iter()
        .any(|p| kind(p) == kind(c) && key(p).is_some() && key(p) == key(c))
}
