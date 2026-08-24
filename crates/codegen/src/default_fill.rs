//! `@default`, lowered once (#374 step 4).
//!
//! A newly-added required field reaches existing rows by **two different
//! routes**, and until this module existed they disagreed:
//!
//! | route | who writes it | what it wrote |
//! |---|---|---|
//! | reopen backfill (`recover_from_wal`, #92) | the generated app, on `open` | the **type zero** — `""`, `0`, `false` |
//! | offline transformer (#74 Phase 3) | the generated hop | the recorded default |
//!
//! So `status: string @default("pending")` produced `""` in one dir and
//! `"pending"` in the other, for the same schema edit, decided by which command
//! the operator happened to run. That is the "same edit, different data by
//! route" hazard gate 1 named as finding 4, and it is the reason `@default`
//! could not simply be added to the provable set on one side.
//!
//! [`default_fill`] is the ONE definition. It has two lowerings and no third:
//! [`FillValue::json_literal`] (what the transformer's hop inserts into the
//! row's JSON) and `RustGenerator::generate_backfill_appends` (what the reopen
//! backfill appends to the column). A tier-1 test asserts the two agree for
//! every supported type; a tier-2 test runs both routes over the same edit.
//!
//! # What is deliberately NOT supported
//!
//! [`default_fill`] returns `None` — meaning "the differ cannot prove this
//! field's value, ask a human" — for:
//!
//! * a **nullable** field. Its zero is `None`, which is already a meaningful
//!   value, and such an add is provable without any default at all.
//! * `timestamp`, `uuid`, `bytes(N)`, `string(N)`, a fixed array, an inline
//!   struct, and a relation. Each has an encoding whose default spelling is not
//!   settled (what is `@default` on a `bytes(4)`?), and inventing one here would
//!   be inventing schema syntax in a codegen helper.
//! * a `@default` whose literal does not fit the field — `@default("x")` on a
//!   `u32`, or a negative on an unsigned. Returning `None` routes it to the
//!   prompt instead of silently substituting a zero, which is what
//!   `add_field_default_json` used to do.
//!
//! `None` is never silent: it makes the add `Authored`, so `migrate create`
//! asks and `migrate build` refuses an unanswered one.

use forgedb_parser::{ConstraintParam, Field, FieldType, Schema};

/// A `@default` value, resolved against its field's type.
///
/// It carries the **value**, never a rendering. Both lowerings derive from it,
/// which is the whole point: a second rendering computed independently is how
/// the two routes disagreed in the first place.
#[derive(Debug, Clone, PartialEq)]
pub enum FillValue {
    Bool(bool),
    /// A signed integer that has already been range-checked against the
    /// field's own integer type.
    Int(i64),
    Float(f64),
    /// A bare `string` value.
    Str(String),
    /// Raw JSON source for a `json` field, already validated as parseable.
    Json(String),
    /// A declared enum variant: its name (the JSON form) and its **positional
    /// discriminant** (the stored byte). Both are carried because the two
    /// routes need different ones and neither may re-derive the other.
    Enum { variant: String, discriminant: u8 },
    /// A `decimal` literal, verbatim, already validated as parseable.
    Decimal(String),
}

impl FillValue {
    /// The JSON literal the transformer's hop inserts into a row (#74 Phase 3).
    ///
    /// This is a *literal source string*, not a `serde_json::Value`: the
    /// generated hop bakes it into `serde_json::from_str(#json)`.
    pub fn json_literal(&self) -> String {
        match self {
            FillValue::Bool(b) => b.to_string(),
            FillValue::Int(i) => i.to_string(),
            FillValue::Float(f) => {
                // `1` is not valid JSON for an f64 field's serde repr in every
                // reader, and `1.0` round-trips everywhere.
                let s = f.to_string();
                if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
                    s
                } else {
                    format!("{s}.0")
                }
            }
            FillValue::Str(s) => json_string(s),
            FillValue::Json(raw) => raw.clone(),
            // An enum crosses every ForgeDB wire as its variant NAME.
            FillValue::Enum { variant, .. } => json_string(variant),
            // `decimal` has string serde so precision survives.
            FillValue::Decimal(d) => json_string(d),
        }
    }
}

/// `-?digits(.digits)?` — the shape `rust_decimal` accepts, checked without
/// linking it. A lexeme this admits and `Decimal::from_str` rejects (an absurd
/// scale, say) fails at the generated crate's `.expect`, loudly, in the one
/// place that has the real type.
fn is_decimal_lexeme(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let mut parts = body.split('.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next();
    if parts.next().is_some() || int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    match frac {
        None => true,
        Some(f) => !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()),
    }
}

/// Quote a Rust string as a JSON string literal.
fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// The `@default` directive's single value parameter, if the field has one.
///
/// `@default` takes exactly one positional argument. A named, exclusive or
/// multi-argument form is not a default anyone wrote on purpose, so it resolves
/// to `None` and the add reaches a human.
fn default_param(field: &Field) -> Option<&ConstraintParam> {
    let c = field
        .constraints
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("default"))?;
    match c.params.as_slice() {
        [only @ (ConstraintParam::Number(_)
        | ConstraintParam::Fractional(_)
        | ConstraintParam::String(_))] => Some(only),
        _ => None,
    }
}

/// Resolve a field's `@default` into the value BOTH routes write, or `None`
/// when the differ cannot prove one (see the module docs for the exact set).
///
/// `schema` is needed to resolve an enum's positional discriminant and to see
/// through an FK to its identity type.
pub fn default_fill(schema: &Schema, field: &Field) -> Option<FillValue> {
    // A nullable field's zero is `None` — already meaningful, and such an add
    // is provable with no default at all. Honouring `@default` here would
    // change what an existing nullable add backfills, for no correctness gain.
    if field.is_nullable() {
        return None;
    }
    let param = default_param(field)?;
    let ty = crate::rust::RustGenerator::resolved_type(schema, &field.field_type);

    match &ty {
        FieldType::Bool => match param {
            ConstraintParam::String(s) if s == "true" => Some(FillValue::Bool(true)),
            ConstraintParam::String(s) if s == "false" => Some(FillValue::Bool(false)),
            ConstraintParam::Number(1) => Some(FillValue::Bool(true)),
            ConstraintParam::Number(0) => Some(FillValue::Bool(false)),
            _ => None,
        },
        FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => {
            let n = match param {
                ConstraintParam::Number(n) => *n,
                ConstraintParam::String(s) => s.parse::<i64>().ok()?,
                _ => return None,
            };
            // Range-check against the field's OWN width. `@default(-1)` on a
            // `u32` is a mistake, and generating `append_u32(-1)` would be a
            // compile error in the user's cache — a failure they cannot read.
            let fits = match ty {
                FieldType::U32 => (0..=u32::MAX as i64).contains(&n),
                FieldType::U64 => n >= 0,
                FieldType::I32 => (i32::MIN as i64..=i32::MAX as i64).contains(&n),
                FieldType::I64 => true,
                _ => unreachable!(),
            };
            fits.then_some(FillValue::Int(n))
        }
        FieldType::F64 => {
            let f = match param {
                ConstraintParam::Number(n) => *n as f64,
                ConstraintParam::Fractional(s) | ConstraintParam::String(s) => {
                    s.parse::<f64>().ok()?
                }
                _ => return None,
            };
            f.is_finite().then_some(FillValue::Float(f))
        }
        FieldType::String => match param {
            ConstraintParam::String(s) => Some(FillValue::Str(s.clone())),
            ConstraintParam::Number(n) => Some(FillValue::Str(n.to_string())),
            ConstraintParam::Fractional(s) => Some(FillValue::Str(s.clone())),
            _ => None,
        },
        FieldType::Json => {
            let raw = match param {
                ConstraintParam::String(s) => s.clone(),
                ConstraintParam::Number(n) => n.to_string(),
                ConstraintParam::Fractional(s) => s.clone(),
                _ => return None,
            };
            // A raw JSON default is used verbatim; a bare word is the string.
            if serde_json::from_str::<serde_json::Value>(&raw).is_ok() {
                Some(FillValue::Json(raw))
            } else {
                Some(FillValue::Json(json_string(&raw)))
            }
        }
        FieldType::Decimal => {
            let raw = match param {
                ConstraintParam::String(s) | ConstraintParam::Fractional(s) => s.clone(),
                ConstraintParam::Number(n) => n.to_string(),
                _ => return None,
            };
            // Validated by shape rather than by parsing through `rust_decimal`:
            // adding that crate to `forgedb-codegen` would put it in the
            // published compiler's dependency graph to check one literal, and
            // the generated code parses the same lexeme with the real type.
            is_decimal_lexeme(&raw).then_some(FillValue::Decimal(raw))
        }
        FieldType::Enum(name) => {
            let ConstraintParam::String(variant) = param else {
                return None;
            };
            let def = schema.find_enum(name)?;
            // The POSITION is the stored byte (#enum): variants map to `0..N` in
            // declaration order. Looked up rather than assumed, and refused when
            // the named variant is not declared.
            let idx = def.variants.iter().position(|v| v == variant)?;
            Some(FillValue::Enum {
                variant: variant.clone(),
                discriminant: u8::try_from(idx).ok()?,
            })
        }
        // Everything else: see the module docs. `None` routes the add to a
        // human rather than to an invented encoding.
        _ => None,
    }
}
