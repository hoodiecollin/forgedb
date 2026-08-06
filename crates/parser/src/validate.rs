//! Positioned, reusable schema validation — the single semantic-diagnostics
//! authority for a parsed [`Schema`].
//!
//! # Why this lives in `forgedb-parser`
//!
//! Schema validation logically belongs next to the *diagnostic vocabulary*
//! (`ValidationError` / `Position`) in [`forgedb-validation`], and epic #173
//! originally proposed extending that crate. But `forgedb-validation`
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
//! **advisory** lints (no timestamp) layer those on top — those are not
//! pure-schema diagnostics and stay in the CLI. (The missing-identity check used
//! to be one of those advisories; #248 made it a hard rule and moved it here, so
//! the LSP reports it too.)

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
    // `@min`/`@max` bound shape vs the field's numeric domain (#239).
    check_numeric_bounds(schema, errors);

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

        // Identity is mandatory (#248). A model with no identity field generates
        // code that does not compile — `create_*` reads `record.id`, `id_to_row`
        // has nothing to key on, and the REST id routes have no path parameter to
        // parse. Beyond compiling, identity is load-bearing for relations,
        // secondary indexes, the change feed, and live queries, so there is very
        // little generated surface left for a model without one.
        //
        // The predicate matches what codegen uses to *find* the identity field, so
        // validation and generation cannot disagree about whether one exists.
        // (Whether the chosen field's *type* can serve as a key is a separate,
        // unenforced question — a `string` or `timestamp` identity still generates
        // code that does not compile. Tracked separately.)
        if !model
            .fields
            .iter()
            .any(|f| f.name == "id" || f.auto_generate)
        {
            errors.push(
                positioned(
                    format!(
                        "Model '{}' has no identity field. Every model needs one: a field named \
                         'id', or any field marked auto-generate ('+').",
                        model.name
                    ),
                    model.position,
                )
                .with_suggestion("id: +uuid"),
            );
        }

        // `&`/`^` on the IDENTITY field is redundant (#258). The identity's
        // uniqueness is already enforced structurally by the generated
        // `id_to_row`, which is a map keyed by id — so codegen deliberately
        // builds no secondary index for it, and the modifier has no effect.
        //
        // This is advisory, never fatal: the schema is valid and generates
        // correct code, the author has just written something that does nothing.
        // Silence would be defensible for a *redundant* modifier; it was NOT
        // defensible for the non-identity case, where the same silence meant no
        // uniqueness enforcement at all (the bug #258 fixes).
        if let Some(identity) = model.fields.iter().find(|f| f.name == "id" || f.auto_generate) {
            if identity.unique || identity.indexed {
                let modifier = if identity.unique { "&" } else { "^" };
                errors.push(
                    positioned(
                        format!(
                            "Field '{}.{}' is the model's identity, so '{}' has no effect — \
                             identity uniqueness is already enforced by the primary key.",
                            model.name, identity.name, modifier
                        ),
                        identity.position.or(model.position),
                    )
                    .with_suggestion(format!("drop the '{}' from '{}'", modifier, identity.name))
                    .with_severity(forgedb_validation::Severity::Warning),
                );
            }
        }

        // An integer auto (`+u32`/`+u64`) must be **conflict-visible** (#187).
        //
        // Unlike `+uuid` (random) and `+timestamp` (the clock), an integer auto
        // allocates from a counter that each process derives for itself. Two
        // writers coordinated through `forgedb coordinate` open the same data dir
        // lock-free, so they can and do allocate the same number. The design does
        // not prevent that — it relies on the collision being *detected*.
        //
        // Detection runs entirely off the opaque write-set the coordinator
        // equality-compares. Exactly two things put a field in it: being the
        // identity (contributes a row key) and carrying `&unique` (contributes a
        // unique-claim key). Either one turns a duplicate into a `Nack`, and the
        // retry re-refreshes past the winner and allocates again.
        //
        // `^` is NOT sufficient, and that is the whole point of the rule: an index
        // makes a value fast to *find*, but claims nothing at commit time, so two
        // processes would both commit and neither would notice.
        //
        // Fatal rather than advisory: the failure it prevents is a silent
        // duplicate in committed data. Supporting the bare shape would require
        // coordinator-side sequence allocation, which would push a schema-shaped
        // concern into schema-agnostic substrate — so it is refused outright.
        let identity_name = model
            .fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
            .map(|f| f.name.as_str());
        for field in &model.fields {
            let is_integer_auto = field.auto_generate
                && matches!(field.field_type, FieldType::U32 | FieldType::U64);
            if !is_integer_auto || field.unique || Some(field.name.as_str()) == identity_name {
                continue;
            }
            errors.push(
                positioned(
                    format!(
                        "Field '{}.{}' is an integer auto-increment that is neither the \
                         model's identity nor unique, so a duplicate allocated by a second \
                         writer process would not be conflict-visible to the commit \
                         coordinator and would commit silently.",
                        model.name, field.name
                    ),
                    field.position.or(model.position),
                )
                .with_suggestion(format!(
                    "mark it unique ('{}: &+{}'), or make it the model's identity",
                    field.name,
                    match field.field_type {
                        FieldType::U32 => "u32",
                        _ => "u64",
                    }
                )),
            );
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

/// Whether a field type is (or wraps) a **discrete** numeric type — one whose
/// values are countable, so that `> n` and `>= n+1` denote the same set.
fn is_discrete_numeric(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => true,
        FieldType::Nullable(inner) => is_discrete_numeric(inner),
        _ => false,
    }
}

/// Whether a field type is (or wraps) a **continuous** numeric type, where a
/// fractional or exclusive bound is meaningful.
fn is_continuous_numeric(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::F64 | FieldType::Decimal => true,
        FieldType::Nullable(inner) => is_continuous_numeric(inner),
        _ => false,
    }
}

/// `@min`/`@max` bound diagnostics (#239).
///
/// Three rules, each rejecting a form that would otherwise be silently
/// misinterpreted rather than loudly refused:
///
/// 1. **Fractional bound on an integer field** (`u32 @min(0.5)`) is an error, not
///    a warning. It is always a confusion — the author meant `1` — and a
///    warning-only path lets the schema keep shipping with a bound that does not
///    mean what it reads as.
/// 2. **Exclusive bound on an integer field** (`u32 @min(>0)`) is an error. On a
///    discrete domain `>0` and `>=1` are the same set, so the operator adds a
///    second spelling without adding expressiveness — the same reasoning that
///    declined operators for `@length` on #235. On a continuous domain there is
///    no equivalent inclusive spelling, which is why the form exists at all.
/// 3. **A mismatched operator** (`@min(<5)`, `@max(>5)`) is an error rather than
///    being read as "exclusive" with the direction ignored.
///
/// Bounds on a non-numeric field are not reported here: `@min` on a `string` is
/// already inert by type, and reporting it belongs with the wider "directive does
/// not apply to this type" rule rather than with bound *shape*.
fn check_numeric_bounds(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        for field in &model.fields {
            let discrete = is_discrete_numeric(&field.field_type);
            let continuous = is_continuous_numeric(&field.field_type);
            if !discrete && !continuous {
                continue;
            }
            for c in &field.constraints {
                let is_min = c.name == "min";
                if !is_min && c.name != "max" {
                    continue;
                }
                for p in &c.params {
                    match p {
                        crate::ast::ConstraintParam::Fractional(lex) if discrete => {
                            errors.push(positioned(
                                format!(
                                    "Field '{}.{}' is an integer type, so @{}({}) cannot be \
                                     fractional — use a whole number",
                                    model.name, field.name, c.name, lex
                                ),
                                field.position,
                            ));
                        }
                        crate::ast::ConstraintParam::Fractional(lex) if is_decimal(&field.field_type) => {
                            check_decimal_representable(
                                lex,
                                &model.name,
                                field,
                                &c.name,
                                errors,
                            );
                        }
                        crate::ast::ConstraintParam::Exclusive { greater, value } => {
                            let op = if *greater { '>' } else { '<' };
                            if discrete {
                                errors.push(positioned(
                                    format!(
                                        "Field '{}.{}' is an integer type, so the exclusive \
                                         bound @{}({}{}) is redundant — shift the value and \
                                         write an inclusive bound instead",
                                        model.name,
                                        field.name,
                                        c.name,
                                        op,
                                        render_bound(value)
                                    ),
                                    field.position,
                                ));
                            } else if *greater != is_min {
                                // `@min` pairs with `>`, `@max` with `<`.
                                let want = if is_min { '>' } else { '<' };
                                errors.push(positioned(
                                    format!(
                                        "Field '{}.{}': @{} takes '{}' for an exclusive bound, \
                                         found '{}'",
                                        model.name, field.name, c.name, want, op
                                    ),
                                    field.position,
                                ));
                            }
                            if let crate::ast::ConstraintParam::Fractional(lex) = value.as_ref() {
                                if discrete {
                                    errors.push(positioned(
                                        format!(
                                            "Field '{}.{}' is an integer type, so @{}({}{}) \
                                             cannot be fractional — use a whole number",
                                            model.name, field.name, c.name, op, lex
                                        ),
                                        field.position,
                                    ));
                                } else if is_decimal(&field.field_type) {
                                    check_decimal_representable(
                                        lex,
                                        &model.name,
                                        field,
                                        &c.name,
                                        errors,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Whether a field type is (or wraps) `decimal`.
fn is_decimal(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::Decimal => true,
        FieldType::Nullable(inner) => is_decimal(inner),
        _ => false,
    }
}

/// `rust_decimal::Decimal` holds a 96-bit mantissa and at most 28 fractional
/// digits. A bound outside that is rejected here rather than in codegen, because
/// codegen constructs the value with `Decimal::from_i128_with_scale`, which
/// **panics** on an out-of-range input — turning an author's typo into a
/// generator crash instead of a schema diagnostic.
fn check_decimal_representable(
    lexeme: &str,
    model_name: &str,
    field: &crate::ast::Field,
    directive: &str,
    errors: &mut Vec<ValidationError>,
) {
    const MAX_SCALE: usize = 28;
    const MAX_MANTISSA: i128 = 79_228_162_514_264_337_593_543_950_335; // 2^96 - 1

    let (int_part, frac_part) = lexeme.split_once('.').unwrap_or((lexeme, ""));
    if frac_part.len() > MAX_SCALE {
        errors.push(positioned(
            format!(
                "Field '{}.{}': @{}({}) has {} fractional digits, but a decimal holds at \
                 most {}",
                model_name,
                field.name,
                directive,
                lexeme,
                frac_part.len(),
                MAX_SCALE
            ),
            field.position,
        ));
        return;
    }
    let digits = format!("{int_part}{frac_part}");
    match digits.parse::<i128>() {
        Ok(m) if m.abs() <= MAX_MANTISSA => {}
        _ => errors.push(positioned(
            format!(
                "Field '{}.{}': @{}({}) is too large to represent as a decimal",
                model_name, field.name, directive, lexeme
            ),
            field.position,
        )),
    }
}

/// Render a bound parameter back to something close to its source spelling, for
/// a diagnostic message.
fn render_bound(p: &crate::ast::ConstraintParam) -> String {
    match p {
        crate::ast::ConstraintParam::Number(n) => n.to_string(),
        crate::ast::ConstraintParam::Fractional(s) => s.clone(),
        other => format!("{other:?}"),
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

    // ---- #248: identity is mandatory ----------------------------------------

    /// A model with no identity field is rejected, positioned at the model, with a
    /// suggestion an editor quick-fix can apply. Before #248 this was a CLI-only
    /// advisory that exited 0 and let uncompilable code be generated.
    #[test]
    fn model_without_identity_is_rejected() {
        // 1: Thing {   2:   name: string   3:   count: u32   4: }
        let schema = ast("Thing {\n  name: string\n  count: u32\n}\n");
        let errors = validate_schema(&schema);
        let identity: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("no identity field"))
            .collect();

        assert_eq!(identity.len(), 1, "exactly one: {errors:?}");
        assert_eq!(
            identity[0].position.map(|p| p.line),
            Some(1),
            "anchored at the model declaration"
        );
        assert_eq!(identity[0].suggestion.as_deref(), Some("id: +uuid"));
        assert!(
            !identity[0].is_warning(),
            "#248 promoted this from an advisory to a hard error"
        );
    }

    /// Both spellings of identity are accepted, and they are the same two the
    /// generators use to *find* the field — validation and codegen must not
    /// disagree about whether a model has one.
    #[test]
    fn either_spelling_of_identity_satisfies_the_rule() {
        for src in [
            "Thing {\n  id: +uuid\n  name: string\n}\n",  // auto-generated `id`
            "Thing {\n  id: uuid\n  name: string\n}\n",   // named `id`, not auto
            "Thing {\n  code: +uuid\n  name: string\n}\n", // auto-generated, other name
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("no identity field")),
                "{src:?} has an identity field: {errors:?}"
            );
        }
    }

    /// The rule is per-model, not per-schema: one good model does not excuse a bad
    /// sibling, and each is reported at its own position.
    #[test]
    fn identity_is_checked_per_model() {
        let schema = ast("Good {\n  id: +uuid\n}\n\nBad {\n  name: string\n}\n");
        let identity: Vec<_> = validate_schema(&schema)
            .into_iter()
            .filter(|e| e.message.contains("no identity field"))
            .collect();
        assert_eq!(identity.len(), 1, "only the bad one: {identity:?}");
        assert!(identity[0].message.contains("'Bad'"));
    }

    /// `&`/`^` on the identity field is redundant, so it warns rather than being
    /// silently accepted (#258). Advisory only — the schema stays valid.
    #[test]
    fn redundant_modifier_on_identity_warns() {
        for (src, modifier, field) in [
            ("Thing {\n  id: &+uuid\n  name: string\n}\n", "&", "id"),
            ("Thing {\n  id: ^uuid\n  name: string\n}\n", "^", "id"),
            ("Thing {\n  code: &+u64\n  name: string\n}\n", "&", "code"),
        ] {
            let errors = validate_schema(&ast(src));
            let redundant: Vec<_> =
                errors.iter().filter(|e| e.message.contains("has no effect")).collect();

            assert_eq!(redundant.len(), 1, "exactly one for {src:?}: {errors:?}");
            assert!(
                redundant[0].is_warning(),
                "redundancy is advisory, never fatal — {src:?}"
            );
            assert!(redundant[0].message.contains(&format!("'{modifier}'")));
            assert!(redundant[0].message.contains(&format!("'Thing.{field}'")));
            assert_eq!(
                redundant[0].suggestion.as_deref(),
                Some(format!("drop the '{modifier}' from '{field}'").as_str())
            );
            assert!(
                redundant[0].position.is_some(),
                "positioned at the field so an editor can anchor a quick-fix"
            );
            // Advisory must not fail the schema.
            assert!(
                !errors.iter().any(|e| !e.is_warning()),
                "{src:?} is a VALID schema: {errors:?}"
            );
        }
    }

    /// The warning is for the *identity* only. `&`/`^` on a non-identity field —
    /// including a non-identity auto field — is meaningful and must stay silent
    /// (#258: it now builds a real index and enforces uniqueness).
    #[test]
    fn modifier_on_non_identity_field_does_not_warn() {
        for src in [
            "Thing {\n  id: +uuid\n  ref_id: &+uuid\n}\n", // auto, but not the identity
            "Thing {\n  id: +uuid\n  seen_at: ^+timestamp\n}\n",
            "Thing {\n  id: +uuid\n  email: &string\n}\n", // ordinary field, control
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("has no effect")),
                "{src:?} carries a meaningful modifier: {errors:?}"
            );
        }
    }

    // ---- #187: integer autos must be conflict-visible ------------------------

    /// An integer auto allocates from a **per-process** counter, so two coordinated
    /// writers can hand out the same number. What makes that collision *detected*
    /// rather than silent is the opaque write-set: an identity contributes an id
    /// key, and `&unique` contributes a unique-claim key, so the coordinator
    /// equality-compares and `Nack`s one writer. A bare non-unique integer auto
    /// contributes neither and would duplicate in silence — so it is refused.
    #[test]
    fn bare_non_unique_integer_auto_is_rejected() {
        for (src, field) in [
            ("Thing {\n  id: +uuid\n  seq: +u64\n}\n", "seq"),
            ("Thing {\n  id: +uuid\n  n: +u32\n}\n", "n"),
        ] {
            let errors = validate_schema(&ast(src));
            let seq: Vec<_> = errors
                .iter()
                .filter(|e| e.message.contains("conflict-visible"))
                .collect();

            assert_eq!(seq.len(), 1, "exactly one for {src:?}: {errors:?}");
            assert!(
                !seq[0].is_warning(),
                "a silent duplicate is not something to warn about — {src:?}"
            );
            assert!(
                seq[0].message.contains(&format!("'Thing.{field}'")),
                "names the offending field: {:?}",
                seq[0].message
            );
            assert!(
                seq[0].position.is_some(),
                "positioned at the field so an editor can anchor a quick-fix"
            );
            // Must name BOTH escapes, not merely refuse: the author has to be able
            // to act on it without reading the RFC.
            let suggestion = seq[0].suggestion.as_deref().unwrap_or("");
            assert!(
                suggestion.contains('&') && suggestion.contains("identity"),
                "the fix names both routes (mark it '&', or make it the identity): {suggestion:?}"
            );
        }
    }

    /// The three shapes that satisfy the rule. `^` is deliberately NOT among them:
    /// an index is not a write-set claim, so it does not make a duplicate visible
    /// to the coordinator — which is exactly the distinction this rule turns on.
    #[test]
    fn conflict_visible_integer_autos_are_accepted() {
        for src in [
            "Thing {\n  id: +u64\n  name: string\n}\n", // identity, `id`-named
            "Thing {\n  code: +u64\n  name: string\n}\n", // identity, other name
            "Thing {\n  id: +uuid\n  seq: &+u64\n}\n", // non-identity, but unique
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("conflict-visible")),
                "{src:?} is conflict-visible: {errors:?}"
            );
        }
    }

    /// `^` alone does not satisfy the rule. Pinned separately from the acceptance
    /// cases above because it is the plausible-looking near-miss: it *reads* like
    /// it enforces something, and an index does make the value fast to look up —
    /// but it claims nothing in the write-set, so two processes still both commit.
    #[test]
    fn indexed_only_integer_auto_is_still_rejected() {
        let src = "Thing {\n  id: +uuid\n  seq: ^+u64\n}\n";
        let errors = validate_schema(&ast(src));
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("conflict-visible") && !e.is_warning()),
            "'^' is not a write-set claim and must not satisfy the rule: {errors:?}"
        );
    }

    /// The rule is about *integer* autos only. `+uuid` draws from a random space
    /// and `+timestamp` from the clock — neither allocates from a shared counter,
    /// so neither needs to be conflict-visible.
    #[test]
    fn non_integer_autos_are_unaffected() {
        for src in [
            "Thing {\n  id: +uuid\n  ref_id: +uuid\n}\n",
            "Thing {\n  id: +uuid\n  seen_at: +timestamp\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("conflict-visible")),
                "{src:?} allocates from no counter: {errors:?}"
            );
        }
    }

    /// The new fatal rule and the #258 redundancy warning must not collide on the
    /// same field. An `id`-named integer identity marked `&` is *redundant* (warn,
    /// stay valid) — it must NOT also trip the #187 rule, and a non-identity
    /// `&+u64` must warn about nothing at all now that `&` is load-bearing there.
    #[test]
    fn integer_auto_rule_and_redundancy_warning_do_not_collide() {
        let redundant = validate_schema(&ast("Thing {\n  id: &+u64\n  name: string\n}\n"));
        assert!(
            redundant.iter().any(|e| e.message.contains("has no effect") && e.is_warning()),
            "an identity's '&' is still redundant (#258): {redundant:?}"
        );
        assert!(
            !redundant.iter().any(|e| !e.is_warning()),
            "redundancy stays advisory — the schema is valid: {redundant:?}"
        );

        let required = validate_schema(&ast("Thing {\n  id: +uuid\n  seq: &+u64\n}\n"));
        assert!(
            required.is_empty(),
            "'&' on a non-identity integer auto is REQUIRED, so it warns about \
             nothing and errors about nothing: {required:?}"
        );
    }
}
