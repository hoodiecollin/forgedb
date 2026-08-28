use crate::ast::{FieldType, RelationType, Schema};
use forgedb_validation::{is_pascal_case, validate_field_name, validate_model_name, ValidationError};

pub fn validate_schema(schema: &Schema) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    collect_naming_errors(schema, &mut errors);
    collect_structure_errors(schema, &mut errors);
    errors
}

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

pub fn collect_structure_errors(schema: &Schema, errors: &mut Vec<ValidationError>) {
    check_numeric_bounds(schema, errors);

    check_inline_strings(schema, errors);

    check_identity_types(schema, errors);

    check_identity_cycles(schema, errors);
    check_fk_key_widths(schema, errors);

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

        if !model.has_identity()
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

        if let Some(identity) = model.identity_field() {
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

        if let Some(identity) = model.identity_field() {
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

fn is_discrete_numeric(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => true,
        FieldType::Nullable(inner) => is_discrete_numeric(inner),
        _ => false,
    }
}

fn is_continuous_numeric(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::F64 | FieldType::Decimal => true,
        FieldType::Nullable(inner) => is_continuous_numeric(inner),
        _ => false,
    }
}

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

const INLINE_STRING_ADVISORY_WIDTH: u8 = 64;

fn direct_inline_string(field_type: &FieldType) -> Option<(u8, bool)> {
    match field_type {
        FieldType::StringN { chars, exact } => Some((*chars, *exact)),
        FieldType::Nullable(inner) => direct_inline_string(inner),
        _ => None,
    }
}

fn nested_inline_string(field_type: &FieldType) -> Option<(u8, bool)> {
    match field_type {
        FieldType::StringN { chars, exact } => Some((*chars, *exact)),
        FieldType::Nullable(inner) | FieldType::FixedArray(inner, _) => nested_inline_string(inner),
        _ => None,
    }
}

fn spell_inline_string(chars: u8, exact: bool) -> String {
    format!("string({}{})", chars, if exact { "!" } else { "" })
}

fn inline_string_spelling(field_type: &FieldType) -> Option<String> {
    nested_inline_string(field_type).map(|(c, e)| spell_inline_string(c, e))
}

fn inline_string_embedding(subject: &str, spelling: &str, field_type: &FieldType) -> String {
    let chars = nested_inline_string(field_type).map(|(c, _)| c).unwrap_or(0);
    format!(
        "{subject} is `{spelling}`, but a by-value container stores its fields as the Rust \
         value's bytes and an inline string materialises as a heap `String` — embedding one \
         would persist a pointer, not the text. Use `bytes({chars})` for fixed-size bytes there."
    )
}

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

fn check_inline_string_directives(
    subject: &str,
    chars: u8,
    exact: bool,
    field: &crate::ast::Field,
    errors: &mut Vec<ValidationError>,
) {
    use crate::ast::ConstraintParam;
    let ty = spell_inline_string(chars, exact);

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

fn check_inline_strings(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        for field in &model.fields {
            let subject = format!("Field '{}.{}'", model.name, field.name);
            check_utf8_placement(&subject, field, errors);

            let direct = direct_inline_string(&field.field_type);

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

            check_inline_string_directives(&subject, chars, exact, field, errors);
            warn_wide_inline_string(&subject, chars, field.position, errors);
        }
    }

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

fn spell_field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::U32 => "u32".into(),
        FieldType::U64 => "u64".into(),
        FieldType::I32 => "i32".into(),
        FieldType::I64 => "i64".into(),
        FieldType::F64 => "f64".into(),
        FieldType::Bool => "bool".into(),
        FieldType::String => "string".into(),
        FieldType::StringN { chars, exact } => spell_inline_string(*chars, *exact),
        FieldType::Json => "json".into(),
        FieldType::Decimal => "decimal".into(),
        FieldType::Uuid => "uuid".into(),
        FieldType::Timestamp(p) => format!("timestamp({})", p.key()),
        FieldType::Enum(name) => format!("{name} (an enum)"),
        FieldType::Bytes(n) => format!("bytes({n})"),
        FieldType::FixedArray(inner, n) => format!("[{}; {n}]", spell_field_type(inner)),
        FieldType::StructType(name) => format!("{name} (a struct)"),
        FieldType::OptionalStructType(name) => format!("{name}? (a struct)"),
        FieldType::Nullable(inner) => format!("{}?", spell_field_type(inner)),
        FieldType::Relation(RelationType::RequiredReference(t)) => format!("*{t}"),
        FieldType::Relation(RelationType::OptionalReference(t)) => format!("?{t}"),
        FieldType::Relation(RelationType::OneToMany(t)) => format!("[{t}]"),
        FieldType::Relation(RelationType::ManyToMany(t)) => format!("[{t}]"),
        FieldType::Component(_) => "a component reference".into(),
    }
}

const ALLOWED_IDENTITY_TYPES: &str =
    "`uuid`, `u32`, `u64`, `i32`, `i64`, `timestamp`, `string(N)` / `string(N!)`, \
     or a required foreign key (`*Model`)";

fn check_identity_types(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        let Some(field) = model.identity_field() else {
            continue;
        };
        let subject = format!("Field '{}.{}'", model.name, field.name);

        if matches!(
            field.field_type,
            FieldType::Relation(RelationType::RequiredReference(_))
        ) {
            continue;
        }

        if matches!(field.field_type, FieldType::String) {
            errors.push(
                positioned(
                    format!(
                        "{subject} is the model's identity, and a bare `string` cannot be one: \
                         the generated code passes a key by value, which needs a fixed-width \
                         `Copy` type. Declare the width — `string(N)` for at most N characters, \
                         `string(N!)` for exactly N."
                    ),
                    field.position,
                )
                .with_suggestion(format!(
                    "give '{}' a declared width, e.g. `{}: string(26!)`",
                    field.name, field.name
                )),
            );
            continue;
        }

        if !field.field_type.is_identity_key() {
            let because = match &field.field_type {
                FieldType::F64 => {
                    " — a float has no total equality, so it cannot key the row map"
                }
                FieldType::Bool => " — two rows would exhaust the key space",
                FieldType::Bytes(_) => {
                    " — the identifiers that motivate a non-uuid key (ULIDs, nanoids, prefixed \
                     vendor keys, ISINs) are text, so they are `string(N)`"
                }
                FieldType::Json | FieldType::Decimal => {
                    " — it has no single canonical byte form, so equal values could key \
                     different rows"
                }
                FieldType::Relation(RelationType::OptionalReference(_)) => {
                    " — the key would be `Option<K>`, and a nullable identity is not a key"
                }
                FieldType::Relation(_) => " — a collection is not a key",
                FieldType::Nullable(_) | FieldType::OptionalStructType(_) => {
                    " — the key would be `Option<K>`, and a nullable identity is not a key"
                }
                FieldType::FixedArray(..) | FieldType::StructType(_) => {
                    " — a composite value is not a key"
                }
                _ => "",
            };
            errors.push(
                positioned(
                    format!(
                        "{subject} is the model's identity, so its type cannot be `{}`{because}. \
                         An identity must be one of: {ALLOWED_IDENTITY_TYPES}.",
                        spell_field_type(&field.field_type)
                    ),
                    field.position,
                )
                .with_suggestion(format!("{}: +uuid", field.name)),
            );
            continue;
        }

        if let Some((chars, exact)) = direct_inline_string(&field.field_type)
            && field.has_constraint("utf8")
        {
            errors.push(
                positioned(
                    format!(
                        "{subject} is the model's identity, so `@utf8` cannot apply to it: an \
                         identity's value must survive a URL path segment unencoded, which \
                         admits only ASCII. `@utf8` would reserve {} bytes per row to hold \
                         characters the write path refuses.",
                        chars as usize * 4
                    ),
                    field.position,
                )
                .with_suggestion(format!(
                    "drop `@utf8` from '{}' (it stays `{}`)",
                    field.name,
                    spell_inline_string(chars, exact)
                )),
            );
        }
    }
}

fn is_decimal(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::Decimal => true,
        FieldType::Nullable(inner) => is_decimal(inner),
        _ => false,
    }
}

fn check_decimal_representable(
    lexeme: &str,
    model_name: &str,
    field: &crate::ast::Field,
    directive: &str,
    errors: &mut Vec<ValidationError>,
) {
    const MAX_SCALE: usize = 28;
    const MAX_MANTISSA: i128 = 79_228_162_514_264_337_593_543_950_335;

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

fn render_bound(p: &crate::ast::ConstraintParam) -> String {
    match p {
        crate::ast::ConstraintParam::Number(n) => n.to_string(),
        crate::ast::ConstraintParam::Fractional(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

fn fk_target(field_type: &FieldType) -> Option<&str> {
    match field_type {
        FieldType::Relation(RelationType::RequiredReference(t))
        | FieldType::Relation(RelationType::OptionalReference(t)) => Some(t.as_str()),
        _ => None,
    }
}

const IDENTITY_CHAIN_LIMIT: usize = 16;

fn resolved_identity_type(schema: &Schema, model: &crate::ast::Model) -> Option<FieldType> {
    let mut current = model;
    for _ in 0..IDENTITY_CHAIN_LIMIT {
        let field = current.identity_field()?;
        match fk_target(&field.field_type) {
            Some(target) => current = schema.models.iter().find(|m| m.name == target)?,
            None => return Some(field.field_type.clone()),
        }
    }
    None
}

fn check_identity_cycles(schema: &Schema, errors: &mut Vec<ValidationError>) {
    for model in &schema.models {
        let mut path: Vec<&str> = vec![model.name.as_str()];
        let mut current = model;
        loop {
            let Some(field) = current.identity_field() else {
                break;
            };
            let Some(target) = fk_target(&field.field_type) else {
                break;
            };
            let Some(next) = schema.models.iter().find(|m| m.name == target) else {
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
                    model.identity_field().and_then(|f| f.position),
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
                    model.identity_field().and_then(|f| f.position),
                ));
                break;
            }
            path.push(next.name.as_str());
            current = next;
        }
    }
}

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

fn positioned(message: String, pos: Option<forgedb_validation::Position>) -> ValidationError {
    let err = ValidationError::new(message);
    match pos {
        Some(p) => err.with_position(p),
        None => err,
    }
}

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

    #[test]
    fn validate_schema_collects_all_errors_with_positions() {
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

    #[test]
    fn validate_schema_accepts_a_valid_schema() {
        let schema = ast("User {\n  id: +uuid\n  email: string\n}\n");
        assert!(validate_schema(&schema).is_empty());
    }

    #[test]
    fn structure_pass_ignores_naming_but_catches_references() {
        let schema = ast("User {\n  id: +uuid\n  BadField: string\n  friend: *Ghost\n}\n");
        let mut errors = Vec::new();
        collect_structure_errors(&schema, &mut errors);
        assert_eq!(errors.len(), 1, "only the structural defect: {errors:?}");
        assert!(errors[0].message.contains("undefined model 'Ghost'"));
    }

    #[test]
    fn model_without_identity_is_rejected() {
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

    #[test]
    fn either_spelling_of_identity_satisfies_the_rule() {
        for src in [
            "Thing {\n  id: +uuid\n  name: string\n}\n",
            "Thing {\n  id: uuid\n  name: string\n}\n",
            "Thing {\n  code: +uuid\n  name: string\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("no identity field")),
                "{src:?} has an identity field: {errors:?}"
            );
        }
    }

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

    #[test]
    fn an_auto_timestamp_identity_at_us_is_accepted() {
        let errors = validate_schema(&ast("Event {\n  id: +timestamp(us)\n  name: string\n}\n"));
        assert!(
            !errors.iter().any(|e| !e.is_warning()),
            "no fatal error: {errors:?}"
        );
    }

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
            stamp_first.models[0].identity_field().map(|f| f.name.as_str()),
            Some("id"),
            "`id` wins by name, regardless of declaration order"
        );
    }

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
            assert!(
                !errors.iter().any(|e| !e.is_warning()),
                "{src:?} is a VALID schema: {errors:?}"
            );
        }
    }

    #[test]
    fn modifier_on_non_identity_field_does_not_warn() {
        for src in [
            "Thing {\n  id: +uuid\n  ref_id: &+uuid\n}\n",
            "Thing {\n  id: +uuid\n  seen_at: ^+timestamp\n}\n",
            "Thing {\n  id: +uuid\n  email: &string\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("has no effect")),
                "{src:?} carries a meaningful modifier: {errors:?}"
            );
        }
    }

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

    #[test]
    fn conflict_visible_integer_autos_are_accepted() {
        for src in [
            "Thing {\n  id: +u64\n  name: string\n}\n",
            "Thing {\n  code: +u64\n  name: string\n}\n",
            "Thing {\n  id: +uuid\n  seq: &+u64\n}\n",
        ] {
            let errors = validate_schema(&ast(src));
            assert!(
                !errors.iter().any(|e| e.message.contains("conflict-visible")),
                "{src:?} is conflict-visible: {errors:?}"
            );
        }
    }

    #[test]
    fn indexed_only_integer_auto_is_accepted() {
        let src = "Thing {\n  id: +uuid\n  seq: ^+u64\n}\n";
        let errors = validate_schema(&ast(src));
        assert!(
            errors.is_empty(),
            "'^' neither grants nor withholds conflict-visibility now: {errors:?}"
        );
    }

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

    #[test]
    fn utf8_on_a_bare_string_is_an_error() {
        let e = errs("T {\n  id: +uuid\n  body: string @utf8\n}\n");
        assert_eq!(e.len(), 1, "exactly one diagnostic: {e:?}");
        assert!(e[0].contains("@utf8"), "{e:?}");
        assert!(e[0].contains("string(N)"), "names where it does apply: {e:?}");

        let e = errs("T {\n  id: +uuid\n  n: u32 @utf8\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");

        assert!(errs("T {\n  id: +uuid\n  t: string(8) @utf8\n}\n").is_empty());
    }

    #[test]
    fn upper_bound_directives_are_rejected_on_an_inline_string() {
        for ok in ["@min(3)", "@length(min: 3)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string(64) {ok}\n}}\n");
            assert!(errs(&src).is_empty(), "`{ok}` is allowed: {:?}", errs(&src));
        }
        for bad in ["@max(40)", "@max(64)", "@max(100)", "@length(max: 40)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string(64) {bad}\n}}\n");
            let e = errs(&src);
            assert_eq!(e.len(), 1, "`{bad}` is one error: {e:?}");
            assert!(e[0].contains("string(64)"), "names the width that already bounds it: {e:?}");
        }
        let e = errs("T {\n  id: +uuid\n  f: string(64) @length(40)\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("string(40!)"), "names the direct spelling: {e:?}");
        let e = errs("T {\n  id: +uuid\n  f: string(64) @length(3, 40)\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");

        for bad in ["@min(3)", "@max(3)", "@length(3)", "@length(min: 3)", "@length(max: 3)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string(26!) {bad}\n}}\n");
            let e = errs(&src);
            assert_eq!(e.len(), 1, "`{bad}` on the exact form is one error: {e:?}");
        }

        for ok in ["@min(3)", "@max(40)", "@length(40)", "@length(min: 1, max: 9)"] {
            let src = format!("T {{\n  id: +uuid\n  f: string {ok}\n}}\n");
            assert!(errs(&src).is_empty(), "bare `string` keeps `{ok}`: {:?}", errs(&src));
        }
    }

    #[test]
    fn a_wide_inline_string_warns_and_still_generates() {
        assert!(warns("T {\n  id: +uuid\n  f: string(64)\n}\n").is_empty());

        let w = warns("T {\n  id: +uuid\n  f: string(120)\n}\n");
        assert_eq!(w.len(), 1, "exactly one advisory: {w:?}");
        assert!(w[0].is_warning(), "a width advisory is never an error");
        assert_eq!(w[0].position.map(|p| p.line), Some(3), "positioned at the field");
        assert!(w[0].message.contains("120"), "names the declared width: {:?}", w[0]);
        assert!(errs("T {\n  id: +uuid\n  f: string(120)\n}\n").is_empty());
    }

    #[test]
    fn an_inline_string_cannot_be_embedded() {
        let e = errs("struct P {\n  code: string(4!)\n}\n\nT {\n  id: +uuid\n  p: P\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("bytes("), "names the fixed-size alternative: {e:?}");

        let e = errs("T {\n  id: +uuid\n  f: [string(4!); 3]\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("bytes("), "{e:?}");
    }

    #[test]
    fn both_inline_string_spellings_are_legal_identities() {
        for src in [
            "Doc {\n  id: string(26!)\n  title: string\n}\n",
            "Customer {\n  id: string(32)\n  name: string\n}\n",
            "Person {\n  id: string(254)\n  name: string\n}\n",
        ] {
            let e = errs(src);
            assert!(e.is_empty(), "a declared-width key is a legal identity: {e:?}");
        }
        assert!(errs("T {\n  id: +uuid\n  code: string(26!)\n}\n").is_empty());
    }

    #[test]
    fn a_bare_string_identity_is_rejected_with_a_width_to_write() {
        let d = diags("Doc {\n  id: string\n  title: string\n}\n");
        let e: Vec<_> = d.iter().filter(|x| !x.is_warning()).collect();
        assert_eq!(e.len(), 1, "exactly one diagnostic: {d:?}");
        assert!(e[0].message.contains("Doc.id"), "names the field: {e:?}");
        assert!(
            e[0].message.contains("Declare the width"),
            "and says what to write instead: {e:?}"
        );
        assert!(e[0].position.is_some(), "positioned: {e:?}");
        assert!(
            e[0]
                .suggestion
                .as_deref()
                .is_some_and(|s| s.contains("declared width")),
            "the fix offered is a width, not `+uuid`: {e:?}"
        );
    }

    #[test]
    fn utf8_on_an_identity_is_an_error() {
        let e = errs("Doc {\n  id: string(26!) @utf8\n  title: string\n}\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("@utf8"), "{e:?}");
        assert!(
            e[0].contains("identity"),
            "and says it is the identity that bars it: {e:?}"
        );
        assert!(errs("Doc {\n  id: +uuid\n  code: string(26!) @utf8\n}\n").is_empty());
    }

    #[test]
    fn a_string_keyed_model_can_be_a_junction_endpoint() {
        let e = errs(
            "Isin {\n  id: string(12!)\n  name: string\n  funds: [Fund]\n}\n\n\
             Fund {\n  id: +uuid\n  label: string\n  holdings: [Isin]\n}\n",
        );
        assert!(e.is_empty(), "a string key holds a junction: {e:?}");

        let e = errs(
            "A {\n  id: string\n  bs: [B]\n}\n\nB {\n  id: +uuid\n  as: [A]\n}\n",
        );
        assert_eq!(e.len(), 1, "one diagnostic, not two: {e:?}");
    }

    #[test]
    fn a_wide_inline_key_warns_and_still_generates() {
        let d = diags("Person {\n  id: string(254)\n  name: string\n}\n");
        assert!(
            d.iter().all(|x| x.is_warning()),
            "advisory only — it must still generate: {d:?}"
        );
        assert!(
            d.iter().any(|x| x.message.contains("Person.id")),
            "and it names the key that pays: {d:?}"
        );
    }

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

        let d = diags("A {\n  id: *B\n}\n\nB {\n  id: *C\n}\n\nC {\n  id: *A\n}\n");
        assert!(
            d.iter().any(|e| e.message.contains("cycle")),
            "a longer cycle is caught too: {d:?}"
        );
    }

    #[test]
    fn a_self_referential_fk_is_not_a_cycle() {
        assert!(
            validate_schema(&ast("Category {\n  id: +uuid\n  parent: ?Category\n}\n")).is_empty(),
            "a self-FK through a non-identity field terminates at the identity"
        );

        assert!(
            validate_schema(&ast(
                "Customer {\n  id: +uuid\n}\n\nOrder {\n  id: *Customer\n  total: i64\n}\n"
            ))
            .is_empty(),
            "an identity chain that terminates is legal"
        );
    }

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

        let quiet = warns("Customer {\n  id: string(8)\n}\n\nOrder {\n  id: +uuid\n  \
                           customer_ref: *Customer\n}\n");
        assert!(
            !quiet.iter().any(|e| e.message.contains("customer_ref")),
            "a narrow inherited key says nothing: {quiet:?}"
        );

        let conventional = warns(
            "Customer {\n  id: +uuid\n}\n\nOrder {\n  id: +uuid\n  customer_ref: *Customer\n}\n",
        );
        assert!(conventional.is_empty(), "{conventional:?}");
    }

    #[test]
    fn an_id_field_wins_over_an_auto_declared_above_it() {
        let schema = ast("Event {\n  seq: +u64\n  id: u32\n  note: string\n}\n");
        let id = schema.models[0]
            .identity_field()
            .expect("the model has an identity");
        assert_eq!(id.name, "id", "`id` wins by name, not by declaration order");
        assert!(matches!(id.field_type, FieldType::U32), "{:?}", id.field_type);

        let schema = ast("Event {\n  id: u32\n  seq: +u64\n  note: string\n}\n");
        assert_eq!(schema.models[0].identity_field().map(|f| f.name.as_str()), Some("id"));

        let schema = ast("Event {\n  code: +uuid\n  note: string\n}\n");
        assert_eq!(schema.models[0].identity_field().map(|f| f.name.as_str()), Some("code"));

        let schema = ast("Event {\n  note: string\n}\n");
        assert!(schema.models[0].identity_field().is_none());
        assert!(!schema.models[0].has_identity());
    }

    #[test]
    fn an_auto_timestamp_reached_by_accident_names_the_stamp() {
        let e = errs("OnlyAutoTimestamp {\n  created_at: +timestamp(us)\n  name: string\n}\n");
        assert_eq!(e.len(), 1, "one mistake, one diagnostic: {e:?}");
        assert!(e[0].contains("created_at"), "names the field, not the model: {e:?}");
        assert!(
            e[0].contains("stamp") && e[0].contains("key"),
            "and says what the confusion is: {e:?}"
        );
    }

    #[test]
    fn a_user_supplied_timestamp_identity_is_admitted_at_every_precision() {
        for src in [
            "Tick {\n  id: timestamp\n  v: i64\n}\n",
            "Tick {\n  id: timestamp(s)\n  v: i64\n}\n",
            "Tick {\n  id: timestamp(ms)\n  v: i64\n}\n",
            "Tick {\n  id: timestamp(us)\n  v: i64\n}\n",
        ] {
            assert!(errs(src).is_empty(), "{src} => {:?}", errs(src));
        }
    }

    #[test]
    fn an_allocated_timestamp_identity_is_floored_at_micros_exactly_once() {
        for src in [
            "Tick {\n  id: +timestamp\n  v: i64\n}\n",
            "Tick {\n  id: +timestamp(s)\n  v: i64\n}\n",
            "Tick {\n  id: +timestamp(ms)\n  v: i64\n}\n",
        ] {
            let e = errs(src);
            assert_eq!(e.len(), 1, "exactly one diagnostic for {src}: {e:?}");
            assert!(e[0].contains("us"), "names the floor: {e:?}");
        }
        assert!(errs("Tick {\n  id: +timestamp(us)\n  v: i64\n}\n").is_empty());
    }

    #[test]
    fn every_unsupported_identity_type_is_rejected_by_name() {
        let cases: &[(&str, &str)] = &[
            ("bool", "T {\n  id: bool\n  n: string\n}\n"),
            ("f64", "T {\n  id: f64\n  n: string\n}\n"),
            ("decimal", "T {\n  id: decimal\n  n: string\n}\n"),
            ("json", "T {\n  id: json\n  n: string\n}\n"),
            ("bytes(N)", "T {\n  id: bytes(26)\n  n: string\n}\n"),
            ("fixed array", "T {\n  id: [u32; 4]\n  n: string\n}\n"),
            ("nullable scalar", "T {\n  id: u32?\n  n: string\n}\n"),
            (
                "enum",
                "enum Colour {\n  Red\n  Blue\n}\n\nT {\n  id: Colour\n  n: string\n}\n",
            ),
            (
                "struct",
                "struct Point {\n  x: f64\n  y: f64\n}\n\nT {\n  id: Point\n  n: string\n}\n",
            ),
            (
                "optional FK",
                "Owner {\n  id: +uuid\n}\n\nT {\n  id: ?Owner\n  n: string\n}\n",
            ),
            (
                "one-to-many",
                "Owner {\n  id: +uuid\n  ts: [T]\n}\n\nT {\n  id: [Owner]\n  n: string\n}\n",
            ),
        ];

        for (label, src) in cases {
            let d = diags(src);
            let e: Vec<_> = d.iter().filter(|x| !x.is_warning()).collect();
            assert_eq!(e.len(), 1, "{label}: exactly one diagnostic: {d:?}");
            assert!(
                e[0].message.contains("T.id"),
                "{label}: names the offending field: {:?}",
                e[0]
            );
            assert!(
                e[0].message.contains("uuid") && e[0].message.contains("string(N)"),
                "{label}: names the allowed set: {:?}",
                e[0]
            );
            let lines: Vec<&str> = src.lines().collect();
            let want = lines
                .iter()
                .rposition(|l| l.trim_start().starts_with("id:"))
                .map(|i| i + 1);
            assert_eq!(
                e[0].position.map(|p| p.line),
                want,
                "{label}: on the field's own line: {:?}",
                e[0]
            );
        }
    }

    #[test]
    fn a_required_fk_identity_is_admitted() {
        assert!(
            errs("Customer {\n  id: +uuid\n}\n\nProfile {\n  id: *Customer\n  bio: string\n}\n")
                .is_empty()
        );
        assert!(
            errs(
                "Region {\n  id: i64\n}\n\nStore {\n  id: *Region\n}\n\n\
                 Till {\n  id: *Store\n  label: string\n}\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn every_admitted_identity_type_validates_clean() {
        for src in [
            "T {\n  id: uuid\n  n: string\n}\n",
            "T {\n  id: +uuid\n  n: string\n}\n",
            "T {\n  code: +uuid\n  n: string\n}\n",
            "T {\n  id: u32\n  n: string\n}\n",
            "T {\n  id: +u32\n  n: string\n}\n",
            "T {\n  id: u64\n  n: string\n}\n",
            "T {\n  id: +u64\n  n: string\n}\n",
            "T {\n  seq: +u64\n  n: string\n}\n",
            "T {\n  id: i32\n  n: string\n}\n",
            "T {\n  id: i64\n  n: string\n}\n",
            "T {\n  id: timestamp(us)\n  n: string\n}\n",
            "T {\n  id: +timestamp(us)\n  n: string\n}\n",
            "T {\n  id: string(26!)\n  n: string\n}\n",
            "T {\n  id: string(32)\n  n: string\n}\n",
            "Owner {\n  id: +uuid\n}\n\nT {\n  id: *Owner\n  n: string\n}\n",
        ] {
            assert!(errs(src).is_empty(), "{src} => {:?}", errs(src));
        }
    }

    #[test]
    fn one_bad_identity_yields_one_diagnostic_even_on_a_junction() {
        let e = errs("A {\n  id: json\n  bs: [B]\n}\n\nB {\n  id: +uuid\n  as: [A]\n}\n");
        assert_eq!(e.len(), 1, "one mistake, one message: {e:?}");
        assert!(e[0].contains("A.id"), "and it is the identity message: {e:?}");

        let e = errs("A {\n  id: string\n  bs: [B]\n}\n\nB {\n  id: +uuid\n  as: [A]\n}\n");
        assert_eq!(e.len(), 1, "one mistake, one message: {e:?}");
        assert!(
            e[0].contains("Declare the width"),
            "the width message survives the fold: {e:?}"
        );
    }

    #[test]
    fn every_admitted_scalar_identity_can_hold_a_junction() {
        for ty in [
            FieldType::Uuid,
            FieldType::U32,
            FieldType::U64,
            FieldType::I32,
            FieldType::I64,
            FieldType::Timestamp(crate::ast::TimestampPrecision::Micros),
            FieldType::StringN { chars: 26, exact: true },
        ] {
            assert!(ty.is_identity_key(), "{ty:?} is an admitted identity");
            assert!(ty.is_junction_key(), "{ty:?} must also hold a junction");
        }
        for ty in [FieldType::String, FieldType::F64, FieldType::Bool, FieldType::Json] {
            assert!(!ty.is_identity_key(), "{ty:?} is not an admitted identity");
            assert!(!ty.is_junction_key(), "{ty:?} cannot hold a junction either");
        }
    }
}
