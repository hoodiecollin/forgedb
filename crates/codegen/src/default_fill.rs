use forgedb_parser::{ConstraintParam, Field, FieldType, Schema};

#[derive(Debug, Clone, PartialEq)]
pub enum FillValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Json(String),
    Enum { variant: String, discriminant: u8 },
    Decimal(String),
}

impl FillValue {
    pub fn json_literal(&self) -> String {
        match self {
            FillValue::Bool(b) => b.to_string(),
            FillValue::Int(i) => i.to_string(),
            FillValue::Float(f) => {
                let s = f.to_string();
                if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
                    s
                } else {
                    format!("{s}.0")
                }
            }
            FillValue::Str(s) => json_string(s),
            FillValue::Json(raw) => raw.clone(),
            FillValue::Enum { variant, .. } => json_string(variant),
            FillValue::Decimal(d) => json_string(d),
        }
    }
}

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

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

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

pub fn default_fill(schema: &Schema, field: &Field) -> Option<FillValue> {
    if field.is_nullable() {
        return None;
    }
    fill_from_param(schema, field, default_param(field)?)
}

pub fn fill_from_param(schema: &Schema, field: &Field, param: &ConstraintParam) -> Option<FillValue> {
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
            is_decimal_lexeme(&raw).then_some(FillValue::Decimal(raw))
        }
        FieldType::Enum(name) => {
            let ConstraintParam::String(variant) = param else {
                return None;
            };
            let def = schema.find_enum(name)?;
            let idx = def.variants.iter().position(|v| v == variant)?;
            Some(FillValue::Enum {
                variant: variant.clone(),
                discriminant: u8::try_from(idx).ok()?,
            })
        }
        _ => None,
    }
}
