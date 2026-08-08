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

    // `string(N)` / `string(N!)` / `@utf8` (#238).
    check_inline_strings(schema, errors);

    // #266: an identity that is itself a foreign key must terminate; a foreign
    // key inherits its target's key width; a junction endpoint's key must be one
    // the junction can physically hold.
    check_identity_cycles(schema, errors);
    check_fk_key_widths(schema, errors);
    check_m2m_endpoint_keys(schema, errors);

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
                // An inline string gets its own diagnostic (#238): the generic
                // "variable-length" wording is actively misleading for a type
                // whose *column* is fixed-width. What disqualifies it is the
                // Rust value, not the column — see `inline_string_embedding`.
                if let Some(spelling) = inline_string_spelling(&field.field_type) {
                    errors.push(positioned(
                        inline_string_embedding(
                            &format!("Struct '{}' field '{}'", struct_def.name, field.name),
                            &spelling,
                            &field.field_type,
                        ),
                        field.position,
                    ));
                } else {
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

        // A `+timestamp` is identity-eligible ONLY when the field is named `id`
        // (#254 res 6).  Forced by the corpus, not by taste: 148 of 148
        // `+timestamp` fields in `examples/` are `created_at`-style stamps and not
        // one is a key.  `+` on a timestamp overwhelmingly means "stamp it now",
        // so without this rule a model with no `id` field containing
        // `metrics_at: +timestamp(us)` would silently acquire a timestamp primary
        // key — the same silent mis-key class #251 exists to close.
        //
        // The asymmetry against #187's integer autos is deliberate: the only
        // reason to write `+u32`/`+u64` is to get an allocated sequence, so an
        // auto-integer is unambiguously key-ish.  Do not "clean this up".
        if let Some(identity) = identity_field(model) {
            if let FieldType::Timestamp(precision) = &identity.field_type {
                if identity.auto_generate && identity.name != "id" {
                    errors.push(
                        positioned(
                            format!(
                                "Model '{}' resolves its identity to '{}', an auto-generate \
                                 timestamp. A '+timestamp' is a stamp, not a key, unless the \
                                 field is named 'id' — name it 'id' if it really is the key, \
                                 or add an identity field so this one stays a stamp.",
                                model.name, identity.name
                            ),
                            identity.position,
                        )
                        .with_suggestion("id: +uuid"),
                    );
                } else if identity.auto_generate
                    && *precision != crate::ast::TimestampPrecision::Micros
                {
                    // The `us` floor (#254 res 2).  An allocated key must be
                    // unique, and uniqueness comes from the monotonic allocator
                    // `next = max(now, last + 1)`, never from the clock.  At a
                    // coarser quantum the allocator still guarantees uniqueness but
                    // does it by running the counter ahead of the wall clock — a
                    // second insert inside one second lands a full second in the
                    // future, and the recovery time after a burst is proportional
                    // to the declared unit.  At `us` (which is also the storage
                    // unit) that drift is bounded by the burst rate itself.
                    errors.push(
                        positioned(
                            format!(
                                "Model '{}' declares 'id: +timestamp({})'. An auto-generate \
                                 timestamp identity must be declared 'us': the key is allocated \
                                 monotonically rather than read from the clock, and at a coarser \
                                 quantum a burst pushes allocated keys into the future one \
                                 {} at a time.",
                                model.name,
                                precision.key(),
                                precision.unit_noun(),
                            ),
                            identity.position,
                        )
                        .with_suggestion("id: +timestamp(us)"),
                    );
                }
            }
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
        if let Some(identity) = identity_field(model) {
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

// ---------------------------------------------------------------------------
// #238 — inline fixed-width strings, `string(N)` / `string(N!)`
// ---------------------------------------------------------------------------

/// The character width above which an inline string stops paying for itself.
///
/// Experiment #261 measured a fixed slot against the variable column's 16-byte
/// `(offset, length)` pair across 200 configurations. The slot wins while it is
/// small and loses once it is wide, and 64 is the conservative end of the
/// bracket where the crossover sat. Above it the declaration still *generates* —
/// a `Copy`, fixed-width key can be worth paying for even when the bytes are not
/// (#252) — so this is an advisory, never an error.
const INLINE_STRING_ADVISORY_WIDTH: u8 = 64;

/// The `(chars, exact)` of a field that **is** an inline string, peeling only
/// `?`. A `[string(4!); 3]` is deliberately *not* one of these — see
/// [`nested_inline_string`].
fn direct_inline_string(field_type: &FieldType) -> Option<(u8, bool)> {
    match field_type {
        FieldType::StringN { chars, exact } => Some((*chars, *exact)),
        FieldType::Nullable(inner) => direct_inline_string(inner),
        _ => None,
    }
}

/// The `(chars, exact)` of an inline string reachable through any composite —
/// `?` and `[T; N]`. Used to catch an *embedded* one, which is an error.
fn nested_inline_string(field_type: &FieldType) -> Option<(u8, bool)> {
    match field_type {
        FieldType::StringN { chars, exact } => Some((*chars, *exact)),
        FieldType::Nullable(inner) | FieldType::FixedArray(inner, _) => nested_inline_string(inner),
        _ => None,
    }
}

/// Render an inline string back to its source spelling, for a diagnostic.
fn spell_inline_string(chars: u8, exact: bool) -> String {
    format!("string({}{})", chars, if exact { "!" } else { "" })
}

/// The spelling of the inline string reachable inside a type, if any.
fn inline_string_spelling(field_type: &FieldType) -> Option<String> {
    nested_inline_string(field_type).map(|(c, e)| spell_inline_string(c, e))
}

/// Why an inline string cannot live inside a by-value container.
///
/// The column is fixed-width, so "variable-length" is the wrong reason and the
/// generic struct diagnostic reads as nonsense here. The real reason is the
/// *Rust* value: an inline string materialises as a heap `String`, and a
/// `struct` / `[T; N]` is persisted by writing the Rust value's bytes — which
/// would store a pointer into this process's heap.
fn inline_string_embedding(subject: &str, spelling: &str, field_type: &FieldType) -> String {
    let chars = nested_inline_string(field_type).map(|(c, _)| c).unwrap_or(0);
    format!(
        "{subject} is `{spelling}`, but a by-value container stores its fields as the Rust \
         value's bytes and an inline string materialises as a heap `String` — embedding one \
         would persist a pointer, not the text. Use `bytes({chars})` for fixed-size bytes there."
    )
}

/// Res 8's width advisory, factored out of the field walk on purpose.
///
/// #266 makes a foreign key's type follow its target's identity type, which
/// gives this warning a **second firing site**: an FK that inherits a wide
/// inline key pays the same per-row cost without its author ever writing a
/// width. That walk is a second loop over relation fields calling this same
/// helper — no restructuring of [`check_inline_strings`] required.
///
/// It lives in `validate.rs` rather than in codegen's `column_value_size_expr`
/// because a diagnostic needs a span and a sink, and that helper is a pure
/// `FieldType -> TokenStream` function called three to four times per field.
fn warn_wide_inline_string(
    subject: &str,
    chars: u8,
    position: Option<forgedb_validation::Position>,
    errors: &mut Vec<ValidationError>,
) {
    if chars <= INLINE_STRING_ADVISORY_WIDTH {
        return;
    }
    errors.push(
        positioned(
            format!(
                "{subject}: `string({chars})` reserves a {chars}-character slot in every row. \
                 Above {INLINE_STRING_ADVISORY_WIDTH} characters that generally costs more than \
                 the 16-byte pointer pair a bare `string` uses (#261) — prefer `string` unless \
                 the value has to be a fixed-width key.",
            ),
            position,
        )
        .with_severity(forgedb_validation::Severity::Warning),
    );
}

/// `@utf8` widens each character of an inline string from one byte to four. On
/// anything else there is nothing to widen — a bare `string` is already UTF-8
/// and lives in the variable column, and a non-string has no characters at all —
/// so the directive can only mean its author misunderstood it. Reported rather
/// than ignored, because silence here reads as "the field is now multi-byte".
fn check_utf8_placement(
    subject: &str,
    field: &crate::ast::Field,
    errors: &mut Vec<ValidationError>,
) {
    if field.has_constraint("utf8") && direct_inline_string(&field.field_type).is_none() {
        errors.push(positioned(
            format!(
                "{subject}: @utf8 only applies to an inline `string(N)`, where it widens each \
                 character to four bytes. A bare `string` is already UTF-8, and a non-string \
                 field has no characters to widen."
            ),
            field.position,
        ));
    }
}

/// Res 9: on an inline string the *type* already carries the length bound, so a
/// length directive is either a redundant restatement or a contradiction — there
/// is no reading in which the author benefits from stating both.
///
/// On `string(N)` the width is the maximum, so lower bounds (`@min`,
/// `@length(min:)`) stay meaningful and only upper bounds are refused. On
/// `string(N!)` the length is fully determined, so every length directive goes,
/// lower bounds included. A bare `string` is untouched (res 10).
fn check_inline_string_directives(
    subject: &str,
    chars: u8,
    exact: bool,
    field: &crate::ast::Field,
    errors: &mut Vec<ValidationError>,
) {
    use crate::ast::ConstraintParam;
    let ty = spell_inline_string(chars, exact);

    // One diagnostic per offending *directive*, never per parameter — a
    // `@length(3, 40)` that is wrong in one component is one mistake.
    let second_bound = |directive: String, want: String| {
        positioned(
            format!(
                "{subject}: the width in `{ty}` is already the upper bound, so {directive} \
                 states a second one. Declare the width you mean — `string({want})` — and drop \
                 the directive."
            ),
            field.position,
        )
    };

    for c in &field.constraints {
        if !matches!(c.name.as_str(), "min" | "max" | "length") {
            continue;
        }
        if exact {
            errors.push(positioned(
                format!(
                    "{subject}: `{ty}` fixes the length at exactly {chars} characters, so \
                     @{} can never change the outcome — drop it.",
                    c.name
                ),
                field.position,
            ));
            continue;
        }
        match c.name.as_str() {
            // A lower bound narrows nothing the type already said.
            "min" => {}
            "max" => {
                let want = c.params.first().map(render_bound).unwrap_or_default();
                errors.push(second_bound(format!("@max({want})"), want.clone()));
            }
            "length" => {
                let named_max = c.params.iter().find_map(|p| match p {
                    ConstraintParam::Named { name, value } if name == "max" => Some(value.as_ref()),
                    _ => None,
                });
                let positional: Vec<&ConstraintParam> = c
                    .params
                    .iter()
                    .filter(|p| !matches!(p, ConstraintParam::Named { .. }))
                    .collect();

                if let Some(v) = named_max {
                    let want = render_bound(v);
                    errors.push(second_bound(format!("@length(max: {want})"), want.clone()));
                } else if positional.len() == 1 {
                    // #235: the single-argument form means EXACTLY n, which is
                    // precisely what the `!` spelling says in the type.
                    let n = render_bound(positional[0]);
                    errors.push(positioned(
                        format!(
                            "{subject}: @length({n}) means exactly {n} characters (#235), which \
                             is what `string({n}!)` spells directly — write the type, not the \
                             directive."
                        ),
                        field.position,
                    ));
                } else if positional.len() >= 2 {
                    let want = render_bound(positional[1]);
                    errors.push(second_bound(
                        format!("the max component of @length({}, {want})", render_bound(positional[0])),
                        want.clone(),
                    ));
                }
            }
            _ => {}
        }
    }
}

/// The whole of #238's semantic layer: `@utf8` placement, the length-directive
/// matrix, the width advisory, and the two shapes an inline string may not take
/// (embedded by value, or serving as a model's identity).
fn check_inline_strings(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        for field in &model.fields {
            let subject = format!("Field '{}.{}'", model.name, field.name);
            check_utf8_placement(&subject, field, errors);

            let direct = direct_inline_string(&field.field_type);

            // Reachable only through a by-value container — same rejection the
            // struct walk makes, for the `[T; N]` spelling.
            if direct.is_none()
                && let Some((c, e)) = nested_inline_string(&field.field_type)
            {
                errors.push(positioned(
                    inline_string_embedding(
                        &subject,
                        &spell_inline_string(c, e),
                        &field.field_type,
                    ),
                    field.position,
                ));
                continue;
            }

            let Some((chars, exact)) = direct else {
                continue;
            };

            // An inline-string identity does not generate compilable code yet.
            // The hole is older than #238 — a bare `string` identity has never
            // compiled either, because the generated REST layer parses every
            // non-integer key as a uuid (`ApiGenerator::id_parse_type`). #252
            // closes it with a `Copy` `InlineStr<N>` key; until then this is a
            // loud refusal rather than the silent generation of a crate that
            // will not build.
            if field.name == "id" || field.auto_generate {
                errors.push(positioned(
                    format!(
                        "{subject} is the model's identity, and a `{}` identity does not \
                         generate compilable code yet — the generated API layer parses every \
                         non-integer key as a uuid. Use `+uuid` or an integer auto for now.",
                        spell_inline_string(chars, exact)
                    ),
                    field.position,
                ));
                continue;
            }

            check_inline_string_directives(&subject, chars, exact, field, errors);
            warn_wide_inline_string(&subject, chars, field.position, errors);
        }
    }

    // Struct fields: a `string(N)` there is already rejected by the fixed-size
    // walk in `collect_structure_errors`, so only `@utf8` placement is left.
    for struct_def in &schema.structs {
        for field in &struct_def.fields {
            check_utf8_placement(
                &format!("Struct '{}' field '{}'", struct_def.name, field.name),
                field,
                errors,
            );
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
/// The identity field of a model — named `id`, or any `+` auto-generate field
/// (#248 makes one of the two mandatory).
///
/// **`id` wins by name, then by `+`.**  Written as a two-pass search rather than
/// one `find(|f| f.name == "id" || f.auto_generate)` on purpose (#254): the
/// single-pass form returns whichever comes FIRST in declaration order, so a
/// model writing `created_at: +timestamp` above `id: +uuid` resolves its identity
/// to the stamp.  Before #254 that produced a `Timestamp` id type and the
/// generated crate simply did not compile — loud, if obscure.  #254 makes a
/// `timestamp` identity legal, so the same schema would now compile and silently
/// key every row on its creation stamp.
fn identity_field(model: &crate::ast::Model) -> Option<&crate::ast::Field> {
    model
        .fields
        .iter()
        .find(|f| f.name == "id")
        .or_else(|| model.fields.iter().find(|f| f.auto_generate))
}

/// The FK target named by a `*Model` / `?Model` field, if it is one.
fn fk_target(field_type: &FieldType) -> Option<&str> {
    match field_type {
        FieldType::Relation(RelationType::RequiredReference(t))
        | FieldType::Relation(RelationType::OptionalReference(t)) => Some(t.as_str()),
        _ => None,
    }
}

/// The maximum identity-FK chain the generator will resolve (#266). Mirrors
/// `RustGenerator::FK_RESOLVE_DEPTH`: past this the resolver returns `None` and
/// codegen falls back to `uuid`, so a chain this long has to be a diagnostic
/// rather than silently wrong output.
const IDENTITY_CHAIN_LIMIT: usize = 16;

/// The concrete key a foreign key ultimately backs onto (#266): the target's
/// identity type, resolved through an identity that is itself a foreign key.
/// `None` for a dangling target, a model with no identity, or a cycle — each of
/// which is separately reported.
fn resolved_identity_type(schema: &Schema, model: &crate::ast::Model) -> Option<FieldType> {
    let mut current = model;
    for _ in 0..IDENTITY_CHAIN_LIMIT {
        let field = identity_field(current)?;
        match fk_target(&field.field_type) {
            Some(target) => current = schema.models.iter().find(|m| m.name == target)?,
            None => return Some(field.field_type.clone()),
        }
    }
    None
}

/// An identity that is itself a foreign key resolves to the target's identity —
/// which may in turn be a foreign key (#266). That chain is a **feature**
/// (`Order { id: *Customer }` is a legal shared-key model), but it must
/// terminate: `Left { id: *Right }` / `Right { id: *Left }` parses today, and
/// left alone would send the generator's resolver into an unbounded recursion.
///
/// The resolver is depth-bounded so it cannot overflow the stack, but a bound
/// alone turns the cycle into *silently wrong output* (the fallback key type)
/// rather than a diagnostic. So the cycle is reported here, where the identity
/// field has a source position to point at.
///
/// A self-referential FK on a NON-identity field (`Category { parent: ?Category }`)
/// is untouched: the walk only ever follows identity fields, and `Category`'s
/// identity is a uuid, so it terminates on the first step.
fn check_identity_cycles(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        let mut path: Vec<&str> = vec![model.name.as_str()];
        let mut current = model;
        loop {
            let Some(field) = identity_field(current) else {
                break;
            };
            let Some(target) = fk_target(&field.field_type) else {
                break;
            };
            let Some(next) = schema.models.iter().find(|m| m.name == target) else {
                // A dangling relation target — already reported as its own error.
                break;
            };
            if path.contains(&next.name.as_str()) {
                path.push(next.name.as_str());
                errors.push(positioned(
                    format!(
                        "Identity cycle: {}. An identity field that is itself a foreign key \
                         resolves to the target's identity, so the chain must terminate at a \
                         concrete key type — give one of these models a concrete identity \
                         (e.g. `id: +uuid`).",
                        path.join(" -> ")
                    ),
                    identity_field(model).and_then(|f| f.position),
                ));
                break;
            }
            if path.len() > IDENTITY_CHAIN_LIMIT {
                errors.push(positioned(
                    format!(
                        "Identity chain from '{}' is deeper than {} models. The generator \
                         resolves a foreign key to the key it ultimately backs onto and stops \
                         at that depth; shorten the chain or give '{}' a concrete identity.",
                        model.name, IDENTITY_CHAIN_LIMIT, model.name
                    ),
                    identity_field(model).and_then(|f| f.position),
                ));
                break;
            }
            path.push(next.name.as_str());
            current = next;
        }
    }
}

/// #238 warns about a wide inline string where it is *declared*. #266 gives the
/// same cost a second, invisible home: a foreign key is physically the column its
/// target's identity occupies, so `customer: *Customer` silently pays
/// `Customer`'s key width on every row of `Order` — and its author never wrote a
/// width at all.
///
/// Reported on the CHILD field that pays it, which is the one that can be
/// changed, rather than on the parent declaration (which may be entirely
/// reasonable in isolation).
fn check_fk_key_widths(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        for field in &model.fields {
            let Some(target_name) = fk_target(&field.field_type) else {
                continue;
            };
            let Some(target) = schema.models.iter().find(|m| m.name == target_name) else {
                continue;
            };
            let Some(FieldType::StringN { chars, .. }) = resolved_identity_type(schema, target)
            else {
                continue;
            };
            warn_wide_inline_string(
                &format!(
                    "Field '{}' on model '{}' inherits '{}'s identity width",
                    field.name, model.name, target_name
                ),
                chars,
                field.position,
                errors,
            );
        }
    }
}

/// A many-to-many junction stores each endpoint's id in a fixed-width column,
/// indexes it in a `HashMap`, and frames it in a fixed-width replication record
/// (#266) — so an endpoint key must be fixed-width, hashable and totally
/// equatable (`FieldType::is_junction_key`).
///
/// Before #266 the generator required both endpoints to be uuid-keyed and
/// **silently emitted nothing** otherwise: the schema compiled and the M2M
/// surface simply did not exist. #266 removed that restriction for every key the
/// junction can physically hold; what it cannot hold is reported here instead of
/// disappearing, so the failure mode is a diagnostic rather than a missing
/// feature.
fn check_m2m_endpoint_keys(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for m in schema.detect_many_to_many_relations() {
        for name in [&m.model1, &m.model2] {
            let Some(model) = schema.models.iter().find(|md| &md.name == name) else {
                continue;
            };
            let Some(ty) = resolved_identity_type(schema, model) else {
                continue;
            };
            if ty.is_junction_key() {
                continue;
            }
            errors.push(positioned(
                format!(
                    "Model '{}' cannot be an endpoint of the many-to-many relation \
                     '{}.{}' <-> '{}.{}': its identity is `{}`, and a junction stores each \
                     endpoint's id in a fixed-width, hashable column. Use a `uuid`, integer or \
                     `timestamp` identity.",
                    name,
                    m.model1,
                    m.field1,
                    m.model2,
                    m.field2,
                    format!("{ty:?}"),
                ),
                identity_field(model).and_then(|f| f.position),
            ));
        }
    }
}

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

    // ---- #254: a `+timestamp` identity ---------------------------------------

    /// Scenario 12 — an auto-generate timestamp identity below the `us` floor is
    /// rejected, and the diagnostic names the floor rather than talking about
    /// seconds (the pre-#254 limitation it replaces).
    #[test]
    fn an_auto_timestamp_identity_below_the_us_floor_is_rejected() {
        for (src, key) in [
            ("Event {\n  id: +timestamp\n  name: string\n}\n", "ms"),
            ("Event {\n  id: +timestamp(ms)\n  name: string\n}\n", "ms"),
            ("Event {\n  id: +timestamp(s)\n  name: string\n}\n", "s"),
        ] {
            let errors = validate_schema(&ast(src));
            let floor: Vec<_> = errors
                .iter()
                .filter(|e| e.message.contains("must be declared 'us'"))
                .collect();
            assert_eq!(floor.len(), 1, "exactly one for {src:?}: {errors:?}");
            assert!(!floor[0].is_warning(), "the floor is fatal — {src:?}");
            assert!(
                floor[0].message.contains(&format!("+timestamp({key})")),
                "the diagnostic names what was written: {:?}",
                floor[0].message
            );
            assert_eq!(floor[0].suggestion.as_deref(), Some("id: +timestamp(us)"));
        }
    }

    /// Scenario 13 — `id: +timestamp(us)` is the one accepted form. This is the
    /// whole point of the issue: `+timestamp` stops being rejected outright.
    #[test]
    fn an_auto_timestamp_identity_at_us_is_accepted() {
        let errors = validate_schema(&ast("Event {\n  id: +timestamp(us)\n  name: string\n}\n"));
        assert!(
            !errors.iter().any(|e| !e.is_warning()),
            "no fatal error: {errors:?}"
        );
    }

    /// Scenario 14 — res 6. A `+timestamp` under any other name is a stamp, so a
    /// model whose only auto field is one does NOT silently acquire a timestamp
    /// primary key.
    #[test]
    fn an_auto_timestamp_is_only_identity_eligible_when_named_id() {
        let errors = validate_schema(&ast(
            "Reading {\n  metrics_at: +timestamp(us)\n  value: f64\n}\n",
        ));
        let misuse: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("is a stamp, not a key"))
            .collect();
        assert_eq!(misuse.len(), 1, "exactly one: {errors:?}");
        assert!(!misuse[0].is_warning());
        assert!(misuse[0].message.contains("'metrics_at'"));
        assert_eq!(misuse[0].suggestion.as_deref(), Some("id: +uuid"));
    }

    /// The corpus shape — 148 of 148 `+timestamp` fields are stamps beside a real
    /// identity — must stay clean, INCLUDING when the stamp is declared first.
    /// That ordering is the precedence hazard #254 closes: the single-pass
    /// `find(name == "id" || auto_generate)` would resolve identity to
    /// `created_at` and, now that a timestamp identity compiles, silently key
    /// every row on its creation stamp.
    #[test]
    fn a_stamp_beside_a_real_identity_is_never_the_identity() {
        for src in [
            "Post {\n  id: +uuid\n  created_at: +timestamp\n  title: string\n}\n",
            "Post {\n  created_at: +timestamp\n  id: +uuid\n  title: string\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| !e.is_warning()),
                "{src:?} is a normal schema: {errors:?}"
            );
        }
        let stamp_first = ast("Post {\n  created_at: +timestamp\n  id: +uuid\n}\n");
        assert_eq!(
            identity_field(&stamp_first.models[0]).map(|f| f.name.as_str()),
            Some("id"),
            "`id` wins by name, regardless of declaration order"
        );
    }

    /// A NON-auto timestamp identity is #251's business, not this issue's — it
    /// must not be swept up by either #254 rule.
    #[test]
    fn a_non_auto_timestamp_identity_is_untouched_here() {
        for src in [
            "Event {\n  id: timestamp\n  name: string\n}\n",
            "Event {\n  id: timestamp(s)\n  name: string\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors
                    .iter()
                    .any(|e| e.message.contains("must be declared 'us'")
                        || e.message.contains("is a stamp, not a key")),
                "{src:?} carries no #254 diagnostic: {errors:?}"
            );
        }
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

    // ---- #187/#260: every integer auto shape is conflict-visible -------------

    /// A bare non-unique integer auto is **accepted** (#260).
    ///
    /// #187 refused it, because detection ran entirely off two write-set classes —
    /// the identity's row key and `&unique`'s claim key — and a bare field
    /// contributed neither, so a duplicate allocated by a second coordinated writer
    /// would commit in silence. #260 adds a third class (`b"s" ++ model ++ field ++
    /// value`), so the bare shape claims too and the premise of the refusal is gone.
    ///
    /// Pinned as its own test rather than folded into the acceptance loop below,
    /// because it is the shape the rule used to reject: if a future change
    /// reintroduces the check, this is the test that says why it must not.
    #[test]
    fn bare_non_unique_integer_auto_is_accepted() {
        for src in [
            "Thing {\n  id: +uuid\n  seq: +u64\n}\n",
            "Thing {\n  id: +uuid\n  n: +u32\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                errors.is_empty(),
                "a bare integer auto is conflict-visible via its sequence claim key \
                 (#260) and must validate clean: {src:?} → {errors:?}"
            );
        }
    }

    /// Every integer-auto shape validates, however the field is marked.
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

    /// `^` on an integer auto is accepted (#260).
    ///
    /// Kept as a distinct case because under #187 this was the deliberate
    /// near-miss — an index *reads* like it enforces something while claiming
    /// nothing at commit time. That reasoning was sound and is now simply moot:
    /// the claim comes from the sequence key, not from the index, so whether the
    /// field is indexed no longer bears on validity either way.
    #[test]
    fn indexed_only_integer_auto_is_accepted() {
        let src = "Thing {\n  id: +uuid\n  seq: ^+u64\n}\n";
        let errors = validate_schema(&ast(src));
        assert!(
            errors.is_empty(),
            "'^' neither grants nor withholds conflict-visibility now: {errors:?}"
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

    /// The #258 redundancy warning still fires exactly where it should. An
    /// `id`-named integer identity marked `&` is *redundant* (warn, stay valid); a
    /// non-identity `&+u64` warns about nothing, because there `&` still buys a
    /// real unique index — it is no longer *required* since #260, but it is not
    /// redundant either.
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

    // ---- #238: `string(N)` ---------------------------------------------------

    /// Every diagnostic message on a schema, error or warning.
    fn diags(src: &str) -> Vec<ValidationError> {
        validate_schema(&ast(src))
    }

    fn errs(src: &str) -> Vec<String> {
        diags(src)
            .into_iter()
            .filter(|d| !d.is_warning())
            .map(|d| d.message)
            .collect()
    }

    fn warns(src: &str) -> Vec<ValidationError> {
        diags(src).into_iter().filter(|d| d.is_warning()).collect()
    }

    /// Res 5: `@utf8` widens an inline string's characters. On a bare `string`
    /// there is nothing to widen — variable storage is already UTF-8 — so the
    /// directive can only mean its author misunderstood it.
    #[test]
    fn utf8_on_a_bare_string_is_an_error() {
        let e = errs("T {\n  id: +uuid\n  body: string @utf8\n}\n");
        assert_eq!(e.len(), 1, "exactly one diagnostic: {e:?}");
        assert!(e[0].contains("@utf8"), "{e:?}");
        assert!(e[0].contains("string(N)"), "names where it does apply: {e:?}");

        // ...and on a non-string it is equally meaningless.
        let e = errs("T {\n  id: +uuid\n  n: u32 @utf8\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");

        // On an inline string it is accepted.
        assert!(errs("T {\n  id: +uuid\n  t: string(8) @utf8\n}\n").is_empty());
    }

    /// Res 9, the whole matrix. N *is* the maximum, so a second upper bound is
    /// either a redundant restatement or a contradiction, and there is no reading
    /// where the author benefits from stating two. Lower bounds stay meaningful.
    #[test]
    fn upper_bound_directives_are_rejected_on_an_inline_string() {
        // `string(N)` — lower bounds allowed.
        for ok in ["@min(3)", "@length(min: 3)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string(64) {ok}\n}}\n");
            assert!(errs(&src).is_empty(), "`{ok}` is allowed: {:?}", errs(&src));
        }
        // `string(N)` — upper bounds rejected, at any value.
        for bad in ["@max(40)", "@max(64)", "@max(100)", "@length(max: 40)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string(64) {bad}\n}}\n");
            let e = errs(&src);
            assert_eq!(e.len(), 1, "`{bad}` is one error: {e:?}");
            assert!(e[0].contains("string(64)"), "names the width that already bounds it: {e:?}");
        }
        // `@length(n)` means EXACTLY n (#235), so it duplicates the `!` spelling —
        // and the diagnostic says which one to write.
        let e = errs("T {\n  id: +uuid\n  f: string(64) @length(40)\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("string(40!)"), "names the direct spelling: {e:?}");
        // The positional two-arg form is rejected through its max component.
        let e = errs("T {\n  id: +uuid\n  f: string(64) @length(3, 40)\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");

        // `string(N!)` — the length is fully determined by the type, so EVERY
        // length directive is redundant, lower bounds included.
        for bad in ["@min(3)", "@max(3)", "@length(3)", "@length(min: 3)", "@length(max: 3)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string(26!) {bad}\n}}\n");
            let e = errs(&src);
            assert_eq!(e.len(), 1, "`{bad}` on the exact form is one error: {e:?}");
        }

        // A bare `string` is untouched: all three still allowed (res 10).
        for ok in ["@min(3)", "@max(40)", "@length(40)", "@length(min: 1, max: 9)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string {ok}\n}}\n");
            assert!(errs(&src).is_empty(), "bare `string` keeps `{ok}`: {:?}", errs(&src));
        }
    }

    /// Res 8: above M the declaration WARNS and still generates. #261 shows a wide
    /// slot losing to pointer storage, but a `Copy` key can be worth paying for
    /// anyway (#252), so this informs rather than forbids.
    #[test]
    fn a_wide_inline_string_warns_and_still_generates() {
        // At M it does not warn — the threshold is where it stops winning, and 64
        // is already the conservative end of #261's bracket.
        assert!(warns("T {\n  id: +uuid\n  f: string(64)\n}\n").is_empty());

        let w = warns("T {\n  id: +uuid\n  f: string(120)\n}\n");
        assert_eq!(w.len(), 1, "exactly one advisory: {w:?}");
        assert!(w[0].is_warning(), "a width advisory is never an error");
        assert_eq!(w[0].position.map(|p| p.line), Some(3), "positioned at the field");
        assert!(w[0].message.contains("120"), "names the declared width: {:?}", w[0]);
        // ...and it is not ALSO an error — the schema still generates.
        assert!(errs("T {\n  id: +uuid\n  f: string(120)\n}\n").is_empty());
    }

    /// A `string(N)` cannot be embedded in an inline `struct` or a `[T; N]`.
    /// Both are stored by transmuting the Rust value's bytes, and the Rust value
    /// is a heap `String` — embedding one would persist a pointer. A silent
    /// acceptance here would generate code that compiles and corrupts.
    #[test]
    fn an_inline_string_cannot_be_embedded() {
        let e = errs("struct P {\n  code: string(4!)\n}\n\nT {\n  id: +uuid\n  p: P\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("bytes("), "names the fixed-size alternative: {e:?}");

        let e = errs("T {\n  id: +uuid\n  f: [string(4!); 3]\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("bytes("), "{e:?}");
    }

    /// A `string(N)` identity does not generate compilable code yet — the whole
    /// API layer parses a non-integer key as a uuid. That hole is older than
    /// #238 (a bare `string` identity has never compiled either) and belongs to
    /// #252, which lands a `Copy` key type. Until then this is a loud refusal
    /// rather than a silent generation of a crate that will not build.
    #[test]
    fn an_inline_string_identity_is_refused_for_now() {
        let e = errs("T {\n  id: string(26!)\n  name: string\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("identity"), "{e:?}");
        // A non-identity inline string in the same model is fine.
        assert!(errs("T {\n  id: +uuid\n  code: string(26!)\n}\n").is_empty());
    }

    // ---- #266: FKs follow the target's identity type -------------------------

    /// **Scenario 15.** An identity that is itself a foreign key makes the
    /// codegen resolver mutually recursive with `id_type_tokens`. `Left { id:
    /// *Right }` / `Right { id: *Left }` parses and validates cleanly today and
    /// generates (wrongly, as `Uuid` on both sides); once the FK arm resolves
    /// through the target it would exhaust the generator's stack with no
    /// diagnostic at all.
    ///
    /// So the cycle is reported HERE, where there is a span, rather than left to
    /// crash there.
    #[test]
    fn an_identity_fk_cycle_is_a_positioned_error() {
        let d = diags("Left {\n  id: *Right\n}\n\nRight {\n  id: *Left\n}\n");
        let cycle: Vec<_> = d.iter().filter(|e| e.message.contains("cycle")).collect();
        assert!(!cycle.is_empty(), "the cycle is reported: {d:?}");
        assert!(!cycle[0].is_warning(), "a cycle cannot generate — it is an error");
        assert!(
            cycle[0].position.is_some(),
            "positioned at the offending identity: {:?}",
            cycle[0]
        );
        assert!(
            cycle[0].message.contains("Left") && cycle[0].message.contains("Right"),
            "names both ends of the cycle: {:?}",
            cycle[0]
        );

        // A three-model cycle is caught the same way — the bound is on the walk,
        // not on a two-model special case.
        let d = diags("A {\n  id: *B\n}\n\nB {\n  id: *C\n}\n\nC {\n  id: *A\n}\n");
        assert!(
            d.iter().any(|e| e.message.contains("cycle")),
            "a longer cycle is caught too: {d:?}"
        );
    }

    /// **Scenario 16.** A self-referential FK is legal and must stay legal:
    /// `parent` resolves to `Category`'s *identity*, which is a uuid, and
    /// terminates. This is the scenario that stops scenario 15's fix from being
    /// over-broad.
    #[test]
    fn a_self_referential_fk_is_not_a_cycle() {
        assert!(
            validate_schema(&ast("Category {\n  id: +uuid\n  parent: ?Category\n}\n")).is_empty(),
            "a self-FK through a non-identity field terminates at the identity"
        );

        // A CHAIN through identities is the feature, not the hazard: `Order.id`
        // resolves to `Customer`'s uuid and stops.
        assert!(
            validate_schema(&ast(
                "Customer {\n  id: +uuid\n}\n\nOrder {\n  id: *Customer\n  total: i64\n}\n"
            ))
            .is_empty(),
            "an identity chain that terminates is legal"
        );
    }

    /// **Scenario 17 (resolution 3).** #238's width advisory is checked at the
    /// *declaration*. #266 gives it a second firing site: an FK inherits the
    /// target's key width and pays it on every row of the referencing model,
    /// without its author ever writing a width.
    ///
    /// The fixture leans on `parse_unvalidated` — a `string(N)` identity is
    /// refused by #238's interim check (that refusal is #252's to delete), so
    /// the schema carries an error alongside. The advisory is asserted
    /// independently of it, which is also the point: the warning must survive
    /// once that error goes.
    #[test]
    fn an_fk_to_a_wide_key_warns_on_the_child_that_pays() {
        let src = "Customer {\n  id: string(254)\n}\n\nOrder {\n  id: +uuid\n  \
                   customer_ref: *Customer\n}\n";
        let w = warns(src);
        let fk: Vec<_> = w
            .iter()
            .filter(|e| e.message.contains("customer_ref"))
            .collect();
        assert_eq!(fk.len(), 1, "exactly one advisory on the child: {w:?}");
        assert!(fk[0].is_warning(), "a width advisory is never an error");
        assert_eq!(
            fk[0].position.map(|p| p.line),
            Some(7),
            "positioned at the CHILD field that pays, not the parent declaration"
        );
        assert!(
            fk[0].message.contains("Customer") && fk[0].message.contains("254"),
            "names the target it inherits from and the width: {:?}",
            fk[0]
        );

        // A narrow key is silent — the advisory is about width, not about FKs.
        let quiet = warns("Customer {\n  id: string(8)\n}\n\nOrder {\n  id: +uuid\n  \
                           customer_ref: *Customer\n}\n");
        assert!(
            !quiet.iter().any(|e| e.message.contains("customer_ref")),
            "a narrow inherited key says nothing: {quiet:?}"
        );

        // ...and a uuid key, the convention, says nothing either.
        let conventional = warns(
            "Customer {\n  id: +uuid\n}\n\nOrder {\n  id: +uuid\n  customer_ref: *Customer\n}\n",
        );
        assert!(conventional.is_empty(), "{conventional:?}");
    }
}
