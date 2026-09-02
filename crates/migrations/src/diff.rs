use crate::types::SchemaChange;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSchema {
    pub models: Vec<SimpleModel>,
    pub enums: Vec<SimpleEnum>,
    pub structs: Vec<SimpleStruct>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleModel {
    pub name: String,
    pub fields: Vec<SimpleField>,
    pub composite_indexes: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleEnum {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleStruct {
    pub name: String,
    pub fields: Vec<SimpleField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleType {
    U32,
    U64,
    I32,
    I64,
    F64,
    Bool,
    Str,
    StrN { chars: usize, exact: bool },
    Bytes(usize),
    Uuid,
    Timestamp(u8),
    Json,
    Decimal,
    Enum(String),
    Struct(String),
    Array(Box<SimpleType>, usize),
    Relation(String),
    Collection(String),
    Opaque(String),
}

impl SimpleType {
    fn int_rank(&self) -> Option<(u8, bool)> {
        match self {
            SimpleType::U32 => Some((32, false)),
            SimpleType::U64 => Some((64, false)),
            SimpleType::I32 => Some((32, true)),
            SimpleType::I64 => Some((64, true)),
            _ => None,
        }
    }

    pub fn widens_to(&self, other: &SimpleType) -> bool {
        if self == other {
            return false;
        }
        match (self, other) {
            (a, b) if a.int_rank().is_some() && b.int_rank().is_some() => {
                let (ab, asig) = a.int_rank().unwrap();
                let (bb, bsig) = b.int_rank().unwrap();
                match (asig, bsig) {
                    (false, false) | (true, true) => bb > ab,
                    (false, true) => bb > ab,
                    (true, false) => false,
                }
            }
            (
                SimpleType::StrN {
                    chars: n,
                    exact: ea,
                },
                SimpleType::StrN {
                    chars: m,
                    exact: eb,
                },
            ) => ea == eb && m > n,
            (SimpleType::Timestamp(a), SimpleType::Timestamp(b)) => b > a,
            _ => false,
        }
    }
}

impl std::fmt::Display for SimpleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimpleType::U32 => write!(f, "u32"),
            SimpleType::U64 => write!(f, "u64"),
            SimpleType::I32 => write!(f, "i32"),
            SimpleType::I64 => write!(f, "i64"),
            SimpleType::F64 => write!(f, "f64"),
            SimpleType::Bool => write!(f, "bool"),
            SimpleType::Str => write!(f, "string"),
            SimpleType::StrN { chars, exact } => {
                write!(f, "string({}{})", chars, if *exact { "!" } else { "" })
            }
            SimpleType::Bytes(n) => write!(f, "bytes({n})"),
            SimpleType::Uuid => write!(f, "uuid"),
            SimpleType::Timestamp(q) => write!(
                f,
                "timestamp({})",
                match q {
                    0 => "s",
                    2 => "us",
                    _ => "ms",
                }
            ),
            SimpleType::Json => write!(f, "json"),
            SimpleType::Decimal => write!(f, "decimal"),
            SimpleType::Enum(n) => write!(f, "enum {n}"),
            SimpleType::Struct(n) => write!(f, "struct {n}"),
            SimpleType::Array(inner, n) => write!(f, "[{inner}; {n}]"),
            SimpleType::Relation(m) => write!(f, "*{m}"),
            SimpleType::Collection(m) => write!(f, "[{m}]"),
            SimpleType::Opaque(s) => write!(f, "{s}"),
        }
    }
}

impl std::str::FromStr for SimpleType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(parse_simple_type(s))
    }
}

fn parse_simple_type(s: &str) -> SimpleType {
    let t = s.trim();
    match t {
        "u32" => return SimpleType::U32,
        "u64" => return SimpleType::U64,
        "i32" => return SimpleType::I32,
        "i64" => return SimpleType::I64,
        "f64" => return SimpleType::F64,
        "bool" => return SimpleType::Bool,
        "string" => return SimpleType::Str,
        "uuid" => return SimpleType::Uuid,
        "json" => return SimpleType::Json,
        "decimal" => return SimpleType::Decimal,
        _ => {}
    }
    if let Some(rest) = t.strip_prefix("enum ") {
        return SimpleType::Enum(rest.to_string());
    }
    if let Some(rest) = t.strip_prefix("struct ") {
        return SimpleType::Struct(rest.to_string());
    }
    if let Some(rest) = t.strip_prefix('*') {
        return SimpleType::Relation(rest.to_string());
    }
    if let Some(inner) = t.strip_prefix("timestamp(").and_then(|r| r.strip_suffix(')')) {
        return match inner {
            "s" => SimpleType::Timestamp(0),
            "ms" => SimpleType::Timestamp(1),
            "us" => SimpleType::Timestamp(2),
            _ => SimpleType::Opaque(s.to_string()),
        };
    }
    if let Some(inner) = t.strip_prefix("bytes(").and_then(|r| r.strip_suffix(')')) {
        return match inner.parse::<usize>() {
            Ok(n) => SimpleType::Bytes(n),
            Err(_) => SimpleType::Opaque(s.to_string()),
        };
    }
    if let Some(inner) = t.strip_prefix("string(").and_then(|r| r.strip_suffix(')')) {
        let (digits, exact) = match inner.strip_suffix('!') {
            Some(d) => (d, true),
            None => (inner, false),
        };
        return match digits.parse::<usize>() {
            Ok(chars) => SimpleType::StrN { chars, exact },
            Err(_) => SimpleType::Opaque(s.to_string()),
        };
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return match inner.rfind(';') {
            Some(i) => {
                let (t_src, n_src) = inner.split_at(i);
                match n_src[1..].trim().parse::<usize>() {
                    Ok(n) => SimpleType::Array(Box::new(parse_simple_type(t_src)), n),
                    Err(_) => SimpleType::Opaque(s.to_string()),
                }
            }
            None => SimpleType::Collection(inner.to_string()),
        };
    }
    SimpleType::Opaque(s.to_string())
}

impl serde::Serialize for SimpleType {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for SimpleType {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(parse_simple_type(&s))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleField {
    pub name: String,
    pub ty: SimpleType,
    pub nullable: bool,
    pub unique: bool,
    pub indexed: bool,
    pub index_type: String,
    pub constraints: Vec<SimpleConstraint>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleConstraint {
    pub name: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameProposal {
    Field {
        model_name: String,
        old_name: String,
        new_name: String,
    },
    Model {
        old_name: String,
        new_name: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffResult {
    pub changes: Vec<SchemaChange>,
    pub rename_proposals: Vec<RenameProposal>,
}

pub struct SchemaDiffer;

impl SchemaDiffer {
    pub fn diff(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> DiffResult {
        let mut changes = Vec::new();
        let mut proposals = Vec::new();

        let old_models: BTreeMap<_, _> = old_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();
        let new_models: BTreeMap<_, _> = new_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let removed_model_names: Vec<_> = old_models
            .keys()
            .filter(|k| !new_models.contains_key(*k))
            .cloned()
            .collect();
        let added_model_names: Vec<_> = new_models
            .keys()
            .filter(|k| !old_models.contains_key(*k))
            .cloned()
            .collect();

        if removed_model_names.len() == 1 && added_model_names.len() == 1 {
            let old_name = &removed_model_names[0];
            let new_name = &added_model_names[0];
            if Self::models_structurally_equal(old_models[old_name], new_models[new_name]) {
                proposals.push(RenameProposal::Model {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                });
            }
        }

        for old_model_name in old_models.keys() {
            if !new_models.contains_key(old_model_name) {
                changes.push(SchemaChange::RemoveModel {
                    model_name: old_model_name.clone(),
                });
            }
        }

        for new_model_name in new_models.keys() {
            if !old_models.contains_key(new_model_name) {
                changes.push(SchemaChange::AddModel {
                    model_name: new_model_name.clone(),
                });
            }
        }

        for (model_name, new_model) in new_models.iter() {
            if let Some(old_model) = old_models.get(model_name) {
                let (field_changes, field_proposals) =
                    Self::diff_fields(model_name, old_model, new_model);
                changes.extend(field_changes);
                proposals.extend(field_proposals);

                let index_changes = Self::diff_composite_indexes(model_name, old_model, new_model);
                changes.extend(index_changes);
            }
        }

        changes.extend(Self::diff_enums(old_schema, new_schema));
        changes.extend(Self::diff_structs(old_schema, new_schema));

        DiffResult {
            changes,
            rename_proposals: proposals,
        }
    }

    fn diff_enums(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_enums: BTreeMap<_, _> = old_schema
            .enums
            .iter()
            .map(|e| (e.name.clone(), e))
            .collect();
        let new_enums: BTreeMap<_, _> = new_schema
            .enums
            .iter()
            .map(|e| (e.name.clone(), e))
            .collect();

        for (name, new_enum) in new_enums.iter() {
            let Some(old_enum) = old_enums.get(name) else {
                continue;
            };
            if old_enum.variants == new_enum.variants {
                continue;
            }
            for (model_name, field_name) in Self::dependent_fields(old_schema, new_schema, name) {
                changes.push(SchemaChange::ChangeEnumVariants {
                    model_name,
                    field_name,
                    enum_name: name.clone(),
                    old_variants: old_enum.variants.clone(),
                    new_variants: new_enum.variants.clone(),
                });
            }
        }

        changes
    }

    fn diff_structs(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_structs: BTreeMap<_, _> = old_schema
            .structs
            .iter()
            .map(|s| (s.name.clone(), s))
            .collect();
        let new_structs: BTreeMap<_, _> = new_schema
            .structs
            .iter()
            .map(|s| (s.name.clone(), s))
            .collect();

        for (name, new_struct) in new_structs.iter() {
            let Some(old_struct) = old_structs.get(name) else {
                continue;
            };
            let old_fields = Self::layout(&old_struct.fields);
            let new_fields = Self::layout(&new_struct.fields);
            if old_fields == new_fields {
                continue;
            }
            for (model_name, field_name) in Self::dependent_fields(old_schema, new_schema, name) {
                changes.push(SchemaChange::ChangeStructLayout {
                    model_name,
                    field_name,
                    struct_name: name.clone(),
                    old_fields: old_fields.clone(),
                    new_fields: new_fields.clone(),
                });
            }
        }

        changes
    }

    fn layout(fields: &[SimpleField]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|f| {
                let ty = if f.nullable {
                    format!("{}?", f.ty)
                } else {
                    f.ty.to_string()
                };
                (f.name.clone(), ty)
            })
            .collect()
    }

    fn dependent_fields(
        old_schema: &SimpleSchema,
        new_schema: &SimpleSchema,
        type_name: &str,
    ) -> Vec<(String, String)> {
        let old_models: BTreeMap<_, _> = old_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();
        let new_models: BTreeMap<_, _> = new_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let mut out = Vec::new();
        for (model_name, new_model) in new_models.iter() {
            let Some(old_model) = old_models.get(model_name) else {
                continue;
            };
            let old_field_names: HashSet<&String> =
                old_model.fields.iter().map(|f| &f.name).collect();
            let mut fields: Vec<&SimpleField> = new_model
                .fields
                .iter()
                .filter(|f| old_field_names.contains(&f.name))
                .filter(|f| f.depends_on.iter().any(|d| d == type_name))
                .collect();
            fields.sort_by(|a, b| a.name.cmp(&b.name));
            for f in fields {
                out.push((model_name.clone(), f.name.clone()));
            }
        }
        out
    }

    fn models_structurally_equal(old: &SimpleModel, new: &SimpleModel) -> bool {
        if old.fields.len() != new.fields.len() {
            return false;
        }
        let old_map: HashMap<_, _> = old
            .fields
            .iter()
            .map(|f| (&f.name, (&f.ty, f.nullable)))
            .collect();
        let new_map: HashMap<_, _> = new
            .fields
            .iter()
            .map(|f| (&f.name, (&f.ty, f.nullable)))
            .collect();
        old_map == new_map
    }

    fn diff_fields(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> (Vec<SchemaChange>, Vec<RenameProposal>) {
        let mut changes = Vec::new();
        let mut proposals = Vec::new();

        let old_fields: BTreeMap<_, _> = old_model
            .fields
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();
        let new_fields: BTreeMap<_, _> = new_model
            .fields
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        let removed_field_names: Vec<_> = old_fields
            .keys()
            .filter(|k| !new_fields.contains_key(*k))
            .cloned()
            .collect();
        let added_field_names: Vec<_> = new_fields
            .keys()
            .filter(|k| !old_fields.contains_key(*k))
            .cloned()
            .collect();

        if removed_field_names.len() == 1 && added_field_names.len() == 1 {
            let old_name = &removed_field_names[0];
            let new_name = &added_field_names[0];
            if old_fields[old_name].ty == new_fields[new_name].ty
                && old_fields[old_name].nullable == new_fields[new_name].nullable
            {
                proposals.push(RenameProposal::Field {
                    model_name: model_name.to_string(),
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                });
            }
        }

        for old_field_name in old_fields.keys() {
            if !new_fields.contains_key(old_field_name) {
                changes.push(SchemaChange::RemoveField {
                    model_name: model_name.to_string(),
                    field_name: old_field_name.clone(),
                });
            }
        }

        for (field_name, new_field) in new_fields.iter() {
            if let Some(old_field) = old_fields.get(field_name) {

                if old_field.ty != new_field.ty {
                    changes.push(SchemaChange::ChangeFieldType {
                        model_name: model_name.to_string(),
                        field_name: field_name.clone(),
                        old_type: old_field.ty.clone(),
                        new_type: new_field.ty.clone(),
                        answer: None,
                    });
                }

                if old_field.nullable != new_field.nullable {
                    changes.push(SchemaChange::ChangeFieldNullability {
                        model_name: model_name.to_string(),
                        field_name: field_name.clone(),
                        old_nullable: old_field.nullable,
                        new_nullable: new_field.nullable,
                        answer: None,
                    });
                }

                if old_field.indexed != new_field.indexed {
                    if new_field.indexed {
                        changes.push(SchemaChange::AddIndex {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                            index_type: new_field.index_type.clone(),
                        });
                    } else {
                        changes.push(SchemaChange::RemoveIndex {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                        });
                    }
                }

                if old_field.unique != new_field.unique {
                    if new_field.unique {
                        changes.push(SchemaChange::AddUniqueConstraint {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                        });
                    } else {
                        changes.push(SchemaChange::RemoveUniqueConstraint {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                        });
                    }
                }

                let constraint_changes =
                    Self::diff_constraints(model_name, field_name, old_field, new_field);
                changes.extend(constraint_changes);
            } else {
                changes.push(SchemaChange::AddField {
                    model_name: model_name.to_string(),
                    field_name: field_name.clone(),
                    field_type: new_field.ty.clone(),
                    nullable: new_field.nullable,
                    default_json: None,
                    answer: None,
                });
            }
        }

        (changes, proposals)
    }

    fn diff_constraints(
        model_name: &str,
        field_name: &str,
        old_field: &SimpleField,
        new_field: &SimpleField,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_constraints: BTreeMap<_, _> = old_field
            .constraints
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        let new_constraints: BTreeMap<_, _> = new_field
            .constraints
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        for (constraint_name, _) in old_constraints.iter() {
            if !new_constraints.contains_key(constraint_name) {
                changes.push(SchemaChange::RemoveConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                });
            }
        }

        for (constraint_name, new_constraint) in new_constraints.iter() {
            if let Some(old_constraint) = old_constraints.get(constraint_name) {
                if old_constraint.params != new_constraint.params {
                    changes.push(SchemaChange::RemoveConstraint {
                        model_name: model_name.to_string(),
                        field_name: field_name.to_string(),
                        constraint_name: constraint_name.clone(),
                    });
                    changes.push(SchemaChange::AddConstraint {
                        model_name: model_name.to_string(),
                        field_name: field_name.to_string(),
                        constraint_name: constraint_name.clone(),
                        constraint_params: new_constraint.params.clone(),
                    });
                }
            } else {
                changes.push(SchemaChange::AddConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                    constraint_params: new_constraint.params.clone(),
                });
            }
        }

        changes
    }

    fn diff_composite_indexes(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_indexes: HashSet<_> = old_model.composite_indexes.iter().cloned().collect();
        let new_indexes: HashSet<_> = new_model.composite_indexes.iter().cloned().collect();

        let mut removed: Vec<_> = old_indexes.iter().filter(|idx| !new_indexes.contains(*idx)).collect();
        removed.sort();
        for old_idx in removed {
            changes.push(SchemaChange::RemoveCompositeIndex {
                model_name: model_name.to_string(),
                fields: old_idx.clone(),
            });
        }

        let mut added: Vec<_> = new_indexes.iter().filter(|idx| !old_indexes.contains(*idx)).collect();
        added.sort();
        for new_idx in added {
            changes.push(SchemaChange::AddCompositeIndex {
                model_name: model_name.to_string(),
                fields: new_idx.clone(),
            });
        }

        changes
    }
}
