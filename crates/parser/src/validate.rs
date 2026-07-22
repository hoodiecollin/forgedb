//! Positioned, reusable schema validation — the single semantic-diagnostics
//! authority for a parsed [`Schema`].
//!
//! # Why this lives in `forgedb-parser`
//!
//! Schema validation logically belongs next to the *diagnostic vocabulary*
//! (`ValidationError` / `Position`) in [`forgedb-validation`], and epic #173
//! (WS2b) originally proposed extending that crate. But `forgedb-validation`
//! cannot see the `Schema` AST: `forgedb-parser` already depends on
//! `forgedb-validation` (for `Position`/`ValidationError`), so a reverse
//! dependency would be a cycle. The AST lives here, so the walk that validates
//! it lives here too — reusing the diagnostic types and naming predicates from
//! `forgedb-validation`. The split is: **validation crate = diagnostic types +
//! reusable predicates; parser = the AST and the walk that applies them.**
//!
//! # The single authority
//!
//! [`validate_schema`] is the one implementation of ForgeDB's schema-level
//! semantic rules. It is consumed by:
//! - the parser itself ([`crate::Parser::parse`] runs it fail-fast to preserve
//!   its `Result<Schema, String>` contract), and
//! - the CLI `forgedb validate` command and the LSP, which call it directly on a
//!   [`Parser::parse_unvalidated`](crate::Parser::parse_unvalidated) AST to
//!   surface **all** diagnostics with positions instead of just the first.
//!
//! Everything it reports is a property of the assembled `Schema` (names,
//! duplicates, cross-references, type constraints). Purely *syntactic* errors
//! (unexpected tokens, malformed directives, empty models, composite-index
//! arity) remain fatal in the parser's structural pass and never reach here.
//!
//! Callers that also need **filesystem** checks (component-file existence) or
//! **advisory** lints (no id / no timestamp) layer those on top — those are not
//! pure-schema diagnostics and stay in the CLI.

use crate::ast::{FieldType, RelationType, Schema};
use forgedb_validation::{is_pascal_case, validate_field_name, validate_model_name, ValidationError};

/// Run the full positioned semantic validation of a parsed schema and return
/// **every** diagnostic (naming + structural). Does not fail fast.
///
/// This is the entry point for the CLI and the LSP. The parser runs the same
/// checks internally (see [`collect_naming_errors`] / [`collect_structure_errors`])
/// but reports only the first, to keep its `Result<Schema, String>` contract.
pub fn validate_schema(schema: &Schema) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    collect_naming_errors(schema, &mut errors);
    collect_structure_errors(schema, &mut errors);
    errors
}

/// Naming-convention diagnostics: PascalCase model/struct/enum names and enum
/// variants, snake_case field names.
///
/// Separated from [`collect_structure_errors`] because the parser gates *these*
/// behind its `use_validation` flag (see `Parser::new_with_validation`), while
/// structural checks always run. `validate_schema` runs both.
pub fn collect_naming_errors(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        if let Err(e) = validate_model_name(&model.name, model.position) {
            errors.push(e);
        }
        for field in &model.fields {
            if let Err(e) = validate_field_name(&field.name, field.position) {
                errors.push(e);
            }
        }
    }

    for struct_def in &schema.structs {
        if let Err(e) = validate_model_name(&struct_def.name, struct_def.position) {
            errors.push(e);
        }
        for field in &struct_def.fields {
            if let Err(e) = validate_field_name(&field.name, field.position) {
                errors.push(e);
            }
        }
    }

    for enum_def in &schema.enums {
        if let Err(e) = validate_model_name(&enum_def.name, enum_def.position) {
            errors.push(e);
        }
        for variant in &enum_def.variants {
            // Variants follow the model-name rule (leading uppercase, alphanumerics).
            if !is_pascal_case(variant) {
                let mut err = ValidationError::new(format!(
                    "Enum '{}' variant '{}' must be PascalCase",
                    enum_def.name, variant
                ));
                if let Some(pos) = enum_def.position {
                    err = err.with_position(pos);
                }
                errors.push(err);
            }
        }
    }
}

/// Structural / reference diagnostics: duplicate names (model/struct/enum,
/// fields within a container, enum variants), relation targets resolve,
/// struct/enum type references resolve, structs contain only fixed-size types,
/// and composite-index / projection field references exist.
///
/// These are always run (the parser does not gate them behind `use_validation`).
pub fn collect_structure_errors(schema: &Schema, errors: &mut Vec<ValidationError>) {
    // Duplicate top-level names.
    check_duplicate_names(
        schema.models.iter().map(|m| (m.name.as_str(), m.position)),
        "model",
        errors,
    );
    check_duplicate_names(
        schema.structs.iter().map(|s| (s.name.as_str(), s.position)),
        "struct",
        errors,
    );
    check_duplicate_names(
        schema.enums.iter().map(|e| (e.name.as_str(), e.position)),
        "enum",
        errors,
    );

    // Structs: fixed-size fields only + duplicate field names.
    for struct_def in &schema.structs {
        let mut seen = std::collections::HashSet::new();
        for field in &struct_def.fields {
            if !seen.insert(field.name.as_str()) {
                errors.push(positioned(
                    format!(
                        "Duplicate field name '{}' in struct '{}'",
                        field.name, struct_def.name
                    ),
                    field.position,
                ));
            }
            if !field.field_type.is_fixed_size() {
                errors.push(positioned(
                    format!(
                        "Struct '{}' field '{}' contains variable-length type. Structs can only contain fixed-size types.",
                        struct_def.name, field.name
                    ),
                    field.position,
                ));
            }
        }
    }

    // Enums: duplicate variants.
    for enum_def in &schema.enums {
        let mut seen = std::collections::HashSet::new();
        for variant in &enum_def.variants {
            if !seen.insert(variant.as_str()) {
                errors.push(positioned(
                    format!(
                        "Duplicate variant '{}' in enum '{}'",
                        variant, enum_def.name
                    ),
                    enum_def.position,
                ));
            }
        }
    }

    // Models: duplicate fields, relation/type references, composite-index and
    // projection field references.
    for model in &schema.models {
        let field_names: std::collections::HashSet<&str> =
            model.fields.iter().map(|f| f.name.as_str()).collect();

        let mut seen = std::collections::HashSet::new();
        for field in &model.fields {
            if !seen.insert(field.name.as_str()) {
                errors.push(positioned(
                    format!(
                        "Duplicate field name '{}' in model '{}'",
                        field.name, model.name
                    ),
                    field.position,
                ));
            }

            // Relation targets must reference a declared model.
            if let FieldType::Relation(rel) = &field.field_type {
                let target = match rel {
                    RelationType::OneToMany(t)
                    | RelationType::RequiredReference(t)
                    | RelationType::OptionalReference(t)
                    | RelationType::ManyToMany(t) => t,
                };
                if schema.find_model(target).is_none() {
                    errors.push(positioned(
                        format!(
                            "Model '{}' field '{}' references undefined model '{}'",
                            model.name, field.name, target
                        ),
                        field.position,
                    ));
                }
            }

            // A remaining bare-identifier `StructType` (enum resolution has already
            // rewritten enum references) must name a declared struct or enum.
            if let Some(named) = field.field_type.struct_name()
                && schema.find_struct(named).is_none()
                && schema.find_enum(named).is_none()
            {
                errors.push(positioned(
                    format!(
                        "Model '{}' field '{}' references unknown type '{}' (no such struct or enum)",
                        model.name, field.name, named
                    ),
                    field.position,
                ));
            }
        }

        // Composite indexes reference existing fields.
        for comp_idx in &model.composite_indexes {
            for field_name in &comp_idx.fields {
                if !field_names.contains(field_name.as_str()) {
                    errors.push(positioned(
                        format!(
                            "Composite index in model '{}' references undefined field '{}'",
                            model.name, field_name
                        ),
                        model.position,
                    ));
                }
            }
        }

        // Projections: unique names + referenced fields exist.
        let mut projection_names = std::collections::HashSet::new();
        for proj in &model.projections {
            if !projection_names.insert(proj.name.as_str()) {
                errors.push(positioned(
                    format!(
                        "Duplicate @projection name '{}' in model '{}'",
                        proj.name, model.name
                    ),
                    model.position,
                ));
            }
            for field_name in &proj.fields {
                if !field_names.contains(field_name.as_str()) {
                    errors.push(positioned(
                        format!(
                            "@projection '{}' in model '{}' references undefined field '{}'",
                            proj.name, model.name, field_name
                        ),
                        model.position,
                    ));
                }
            }
        }
    }
}

/// Build a `ValidationError` with an optional position attached.
fn positioned(message: String, pos: Option<forgedb_validation::Position>) -> ValidationError {
    let err = ValidationError::new(message);
    match pos {
        Some(p) => err.with_position(p),
        None => err,
    }
}

/// Report every name that appears more than once (after the first), attaching the
/// duplicate occurrence's position.
fn check_duplicate_names<'a>(
    names: impl Iterator<Item = (&'a str, Option<forgedb_validation::Position>)>,
    kind: &str,
    errors: &mut Vec<ValidationError>,
) {
    let mut seen = std::collections::HashSet::new();
    for (name, pos) in names {
        if !seen.insert(name) {
            errors.push(positioned(
                format!("Duplicate {} name '{}'", kind, name),
                pos,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn ast(input: &str) -> Schema {
        Parser::new(input)
            .unwrap()
            .parse_unvalidated()
            .expect("structural parse should succeed")
    }

    /// The defining capability of the consolidated validator: unlike the parser's
    /// fail-fast `Result<_, String>`, `validate_schema` collects *every* semantic
    /// diagnostic in one pass, each carrying a source position for the LSP.
    #[test]
    fn validate_schema_collects_all_errors_with_positions() {
        // Two independent defects in distinct categories: a snake_case violation
        // (naming) and a dangling relation target (structure).
        let schema = ast("User {\n  id: +uuid\n  BadField: string\n  friend: *Ghost\n}\n");

        let errors = validate_schema(&schema);
        assert_eq!(errors.len(), 2, "both defects reported, not just the first: {errors:?}");

        let naming = errors
            .iter()
            .find(|e| e.message.contains("snake_case"))
            .expect("field-naming diagnostic present");
        assert_eq!(
            naming.position.map(|p| p.line),
            Some(3),
            "naming error points at the offending field's line"
        );

        let dangling = errors
            .iter()
            .find(|e| e.message.contains("references undefined model 'Ghost'"))
            .expect("dangling-relation diagnostic present");
        assert_eq!(
            dangling.position.map(|p| p.line),
            Some(4),
            "relation error points at the offending field's line"
        );
    }

    /// A fully valid schema yields no diagnostics.
    #[test]
    fn validate_schema_accepts_a_valid_schema() {
        let schema = ast("User {\n  id: +uuid\n  email: string\n}\n");
        assert!(validate_schema(&schema).is_empty());
    }

    /// Naming vs. structural split: `collect_structure_errors` alone ignores a
    /// casing violation (that is the parser's `use_validation`-gated concern),
    /// but still reports a dangling relation.
    #[test]
    fn structure_pass_ignores_naming_but_catches_references() {
        let schema = ast("User {\n  id: +uuid\n  BadField: string\n  friend: *Ghost\n}\n");
        let mut errors = Vec::new();
        collect_structure_errors(&schema, &mut errors);
        assert_eq!(errors.len(), 1, "only the structural defect: {errors:?}");
        assert!(errors[0].message.contains("undefined model 'Ghost'"));
    }
}
