use crate::types::SchemaChange;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeMap;

/// Simple schema representation for diffing.
///
/// # Why `enums` and `structs` are here and not only `models` (#438)
///
/// A field's *type* is compared as a string, and for an `enum`/`struct` field
/// that string carries only the **name** (`Enum("Status")`) — the definition
/// lives beside the models, not inside the reference. So a schema that reorders
/// `Status`'s variants produces two `SimpleSchema` values whose models compare
/// **equal**, and the differ, handed two equal values, correctly reports
/// nothing.
///
/// That silence is not cosmetic. An enum is stored as a **positional 1-byte
/// discriminant** (variants map to `0..N` in declaration order), so a reorder
/// re-maps every already-written row to a different variant with no byte on
/// disk changing and no version moving; an inline `struct` is `#[repr(C)]` and
/// every field's offset is a function of the whole declaration. Carrying the
/// ordered definitions here is what lets the differ *see* those edits, record a
/// hop, and bump the schema version that arms the generated open guard.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSchema {
    pub models: Vec<SimpleModel>,
    /// Every declared `enum`, with its variants in **declaration order**. The
    /// order is the payload: it is the byte→meaning map.
    pub enums: Vec<SimpleEnum>,
    /// Every declared inline `struct`, with its fields in **declaration
    /// order**. Order and per-field width are both load-bearing on disk.
    pub structs: Vec<SimpleStruct>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleModel {
    pub name: String,
    pub fields: Vec<SimpleField>,
    pub composite_indexes: Vec<Vec<String>>,
}

/// A declared `enum`, projected for diffing (#438).
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleEnum {
    pub name: String,
    /// Declaration order — this IS the stored discriminant mapping.
    pub variants: Vec<String>,
}

/// A declared inline `struct`, projected for diffing (#438).
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleStruct {
    pub name: String,
    /// Declaration order — this IS the `#[repr(C)]` field layout.
    pub fields: Vec<SimpleField>,
}

/// The differ's own structural type (#374 step 2).
///
/// Deliberately **not** `forgedb_parser::FieldType`: this crate has no parser
/// dependency and must not gain one. The projection from the AST lives in the
/// CLI's `to_simple_schema`, exactly where `SimpleField::depends_on` is already
/// populated (#438), so the differ stays a pure comparison over its own value
/// types.
///
/// # Why a type at all, when a string compared equal
///
/// Equality was all the *differ* needed, and it is not all the *classifier*
/// needs. `hop_body_class` has to answer "is this type change value-preserving?"
/// — `u32 -> u64` maps every value unchanged and there is nothing for a human to
/// decide — and a `String` can only answer "same or not". The whole of #374's
/// direction B (shrink the provable set so authoring is rare by arithmetic)
/// rests on that question being answerable.
///
/// # `Opaque`, and why it is not a hole
///
/// On the wire — inside a committed migration record — a `SimpleType` is its
/// [`Display`] form, a plain string. That keeps every record written before
/// #374 loadable: its `"U32"` (a `format!("{:?}", FieldType::U32)` from the old
/// projection) is not a spelling this grammar produces, so it deserializes as
/// `Opaque("U32")`. `Opaque` widens to nothing and equals only itself, so a
/// legacy hop classifies `Authored` — which is precisely the class it carried on
/// the day it was recorded. The alternative, a structured serde enum, would
/// refuse to *load* those records at all, which is worse than refusing to build
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleType {
    U32,
    U64,
    I32,
    I64,
    F64,
    Bool,
    /// Bare `string` — the variable-width column.
    Str,
    /// `string(N)` / `string(N!)` (#238) — a fixed-width inline string.
    StrN { chars: usize, exact: bool },
    /// `bytes(N)` — raw fixed-size bytes, NOT text.
    Bytes(usize),
    Uuid,
    /// Quantum rank: `0` = `s`, `1` = `ms`, `2` = `us`. Storage is always
    /// microseconds (#254); the rank is the declared quantum, and a **finer**
    /// quantum is a strictly larger rank.
    Timestamp(u8),
    Json,
    Decimal,
    Enum(String),
    Struct(String),
    Array(Box<SimpleType>, usize),
    /// An FK scalar (`*Model` / `?Model`) — a column, unlike a collection.
    Relation(String),
    /// `[Model]` — a pure collection relation. It occupies no column, so
    /// `is_storage_backed` filters it out before it reaches the differ; it
    /// exists here so the AST projection can be **total** rather than needing a
    /// catch-all that would silently absorb a future variant.
    Collection(String),
    /// A type string this build cannot structure.
    ///
    /// Two producers, both honest: a record written **before** #374 (whose
    /// `"U32"` is not a spelling this grammar emits), and a **component
    /// reference**, which names a UI artifact rather than a column and has no
    /// structure to reason about. Both classify `Authored`, which is the class
    /// each carried already.
    Opaque(String),
}

impl SimpleType {
    /// Rank of a numeric type for widening purposes: `(bits, signed)`.
    fn int_rank(&self) -> Option<(u8, bool)> {
        match self {
            SimpleType::U32 => Some((32, false)),
            SimpleType::U64 => Some((64, false)),
            SimpleType::I32 => Some((32, true)),
            SimpleType::I64 => Some((64, true)),
            _ => None,
        }
    }

    /// The ONE definition of a **value-preserving widening** (#374 direction B).
    ///
    /// True only when every value of `self` maps to itself in `other` with
    /// nothing for a human to decide. It is deliberately narrow, and the
    /// negative cases are as load-bearing as the positive ones:
    ///
    /// | change | widens | why |
    /// |---|---|---|
    /// | `u32 -> u64`, `i32 -> i64`, `u32 -> i64` | **yes** | every value representable, sign preserved |
    /// | `u64 -> i64`, `i32 -> u32`, `u32 -> i32` | no | a value at either extreme does not survive |
    /// | any `int -> f64` | no | `f64` cannot represent every `i64`/`u64` exactly, and the *reason* a schema author widens to a float is usually a semantic one |
    /// | `string(N) -> string(M)`, `M > N`, same exactness | **yes** | the slot grows; `!` on both sides or neither |
    /// | `string(N!) -> string(N)` | no | dropping `!` is a constraint change the author may want to police |
    /// | `string(N) -> string` | no | it is a column-kind change (fixed slot to variable column) |
    /// | `timestamp(s) -> timestamp(ms|us)` | **yes** | storage is always microseconds; a finer quantum floors nothing that was not already floored |
    /// | `timestamp(us) -> timestamp(s)` | no | a coarser quantum floors stored values — data loss |
    /// | anything to/from `Opaque` | no | unknown is never provable |
    pub fn widens_to(&self, other: &SimpleType) -> bool {
        if self == other {
            // Not a change at all; the differ never emits one. Answered `false`
            // so this is never mistaken for "the widening set includes identity".
            return false;
        }
        match (self, other) {
            (a, b) if a.int_rank().is_some() && b.int_rank().is_some() => {
                let (ab, asig) = a.int_rank().unwrap();
                let (bb, bsig) = b.int_rank().unwrap();
                match (asig, bsig) {
                    // unsigned -> unsigned, signed -> signed: bits may only grow.
                    (false, false) | (true, true) => bb > ab,
                    // unsigned -> signed: needs a strictly wider target, so the
                    // whole unsigned range still fits.
                    (false, true) => bb > ab,
                    // signed -> unsigned: negatives have nowhere to go.
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
    /// The canonical spelling — and the **only** form that reaches a migration
    /// record, where it is a plain JSON string.
    ///
    /// It is close to the `.forge` surface syntax on purpose: a record is read
    /// by humans deciding whether to trust a migration, and `Nullable(U32)` was
    /// never a spelling anyone typed.
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
    /// Infallible: an unrecognised spelling is [`SimpleType::Opaque`].
    ///
    /// A parse *error* here would turn "this record predates #374" into a load
    /// failure, and the only records carrying an unrecognised spelling are
    /// exactly those.
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(parse_simple_type(s))
    }
}

/// The exact inverse of [`SimpleType`]'s [`Display`], degrading to
/// [`SimpleType::Opaque`] rather than failing.
///
/// Round-tripping is a property, not a hope: `simple_type_round_trips` in this
/// crate's tests renders every variant and parses it back.
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
        // `[T; N]` (fixed array) vs `[Model]` (collection) — the `;` decides.
        // Split from the RIGHT so a nested `[[u32; 2]; 3]` splits on its own
        // separator rather than the inner one.
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
    /// A plain string, so a migration record stays readable and every record
    /// written before #374 stays loadable.
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
    /// The field's **base** type — nullability is `nullable` below and is
    /// carried nowhere else (#374 step 1).
    pub ty: SimpleType,
    pub nullable: bool,
    pub unique: bool,
    pub indexed: bool,
    pub index_type: String,
    pub constraints: Vec<SimpleConstraint>,
    /// The names of every `enum` / `struct` this field's type reaches
    /// **transitively** (#438).
    ///
    /// This is how a definition change is projected back onto the model fields
    /// that store it. Transitivity is the whole point and is easy to get wrong:
    /// an enum can sit inside an inline struct, and a struct inside a struct
    /// (`[Point; 4]` inside `Shape`, `Color` inside `Point`). When `Color`
    /// changes, the outer field's own declaration text does not — so a
    /// dependency list that stops at depth one is the same blindness wearing a
    /// smaller hat.
    ///
    /// Populated where the full AST is (the CLI's `to_simple_schema`), so this
    /// crate stays a pure comparison over its own value types with no parser
    /// dependency.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleConstraint {
    pub name: String,
    pub params: Vec<String>,
}

/// Schema differ - compares two schemas and generates change list
pub struct SchemaDiffer;

impl SchemaDiffer {
    /// Diff two schemas and return the list of changes.
    ///
    /// The returned list is sorted deterministically so that checksums are stable
    /// across runs (M2).
    pub fn diff(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // Build ordered maps (BTreeMap) for deterministic iteration (M2)
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

        // H4: Detect model renames — when exactly one model is removed and one is
        // added AND both have the same set of field names with matching types, treat
        // this as a rename rather than a drop+recreate to avoid data loss.
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

        // Track which models were resolved as renames so they are not also emitted
        // as add/remove.
        let mut renamed_old: HashSet<String> = HashSet::new();
        let mut renamed_new: HashSet<String> = HashSet::new();

        if removed_model_names.len() == 1 && added_model_names.len() == 1 {
            let old_name = &removed_model_names[0];
            let new_name = &added_model_names[0];
            let old_model = old_models[old_name];
            let new_model = new_models[new_name];
            if Self::models_structurally_equal(old_model, new_model) {
                changes.push(SchemaChange::RenameModel {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                });
                renamed_old.insert(old_name.clone());
                renamed_new.insert(new_name.clone());
            }
        }

        // Detect removed models (M2: sorted by key via BTreeMap)
        for old_model_name in old_models.keys() {
            if !new_models.contains_key(old_model_name) && !renamed_old.contains(old_model_name) {
                changes.push(SchemaChange::RemoveModel {
                    model_name: old_model_name.clone(),
                });
            }
        }

        // Detect added models
        for new_model_name in new_models.keys() {
            if !old_models.contains_key(new_model_name) && !renamed_new.contains(new_model_name) {
                changes.push(SchemaChange::AddModel {
                    model_name: new_model_name.clone(),
                });
            }
        }

        // Detect field changes in existing models (M2: sorted by model name via BTreeMap)
        for (model_name, new_model) in new_models.iter() {
            if let Some(old_model) = old_models.get(model_name) {
                let field_changes = Self::diff_fields(model_name, old_model, new_model);
                changes.extend(field_changes);

                let index_changes = Self::diff_composite_indexes(model_name, old_model, new_model);
                changes.extend(index_changes);
            }
        }

        // #438: the definitions themselves. A field's type string carries only
        // the enum/struct NAME, so these edits are invisible to `diff_fields`
        // above by construction — nothing there is wrong, it is comparing two
        // values that are equal.
        changes.extend(Self::diff_enums(old_schema, new_schema));
        changes.extend(Self::diff_structs(old_schema, new_schema));

        changes
    }

    /// Diff the declared `enum`s and project each change onto the model fields
    /// that store it (#438).
    ///
    /// Only an enum present on **both** sides is diffed. A newly-declared enum
    /// has no stored rows, and a removed one is already reported through the
    /// referencing field (which must itself have changed type or gone away).
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

    /// Diff the declared inline `struct`s and project each change onto the model
    /// fields that store one (#438).
    ///
    /// A struct's layout is `(name, type)` per field, in declaration order —
    /// both halves are load-bearing on disk, so both are carried.
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

    /// A struct's on-disk layout as `(field name, field type)` in declaration order.
    ///
    /// The rendered type carries a trailing `?` for a nullable field. Since
    /// #374 step 1 the projected `field_type` has its nullability **stripped**
    /// (it lives in `SimpleField.nullable`), so without re-attaching it here
    /// `Point` and `Point?` inside an inline struct would render identically —
    /// and an optional struct member occupies a discriminant slot the required
    /// one does not, so that edit moves every following field's offset. The one
    /// place the two facts have to be recombined is the place the layout is
    /// named.
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

    /// Every `(model, field)` that stores a value reaching `type_name`, in a
    /// deterministic order (#438).
    ///
    /// Restricted to models AND fields present on **both** sides: a newly-added
    /// model or field has no rows written under the old mapping, so there is
    /// nothing about it to migrate, and a removed one is already reported as its
    /// own change.
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

    /// Return true when two models have the same set of field names and matching types.
    /// Used for rename detection (H4).
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

    /// Diff fields between two models.
    ///
    /// Uses BTreeMap for deterministic ordering (M2).
    /// Detects field renames when exactly one field is removed and one is added
    /// with the same type (H4).
    fn diff_fields(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // M2: BTreeMap gives stable, sorted iteration
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

        // H4: single unambiguous field rename — one removed, one added, same type
        let mut renamed_old_fields: HashSet<String> = HashSet::new();
        let mut renamed_new_fields: HashSet<String> = HashSet::new();

        if removed_field_names.len() == 1 && added_field_names.len() == 1 {
            let old_name = &removed_field_names[0];
            let new_name = &added_field_names[0];
            if old_fields[old_name].ty == new_fields[new_name].ty
                && old_fields[old_name].nullable == new_fields[new_name].nullable
            {
                changes.push(SchemaChange::RenameField {
                    model_name: model_name.to_string(),
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                });
                renamed_old_fields.insert(old_name.clone());
                renamed_new_fields.insert(new_name.clone());
            }
        }

        // Detect removed fields (M2: sorted via BTreeMap)
        for old_field_name in old_fields.keys() {
            if !new_fields.contains_key(old_field_name)
                && !renamed_old_fields.contains(old_field_name)
            {
                changes.push(SchemaChange::RemoveField {
                    model_name: model_name.to_string(),
                    field_name: old_field_name.clone(),
                });
            }
        }

        // Detect added and modified fields (M2: sorted via BTreeMap)
        for (field_name, new_field) in new_fields.iter() {
            if let Some(old_field) = old_fields.get(field_name) {
                // Field exists in both - check for changes

                // Type change
                if old_field.ty != new_field.ty {
                    changes.push(SchemaChange::ChangeFieldType {
                        model_name: model_name.to_string(),
                        field_name: field_name.clone(),
                        old_type: old_field.ty.clone(),
                        new_type: new_field.ty.clone(),
                    });
                }

                // Nullability change
                if old_field.nullable != new_field.nullable {
                    changes.push(SchemaChange::ChangeFieldNullability {
                        model_name: model_name.to_string(),
                        field_name: field_name.clone(),
                        old_nullable: old_field.nullable,
                        new_nullable: new_field.nullable,
                    });
                }

                // Index changes
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

                // Unique constraint changes
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

                // Constraint changes
                let constraint_changes =
                    Self::diff_constraints(model_name, field_name, old_field, new_field);
                changes.extend(constraint_changes);
            } else if !renamed_new_fields.contains(field_name) {
                // New field
                changes.push(SchemaChange::AddField {
                    model_name: model_name.to_string(),
                    field_name: field_name.clone(),
                    field_type: new_field.ty.clone(),
                    nullable: new_field.nullable,
                    default_json: None,
                });
            }
        }

        changes
    }

    /// Diff constraints between two fields.
    ///
    /// Uses BTreeMap for deterministic ordering (M2).
    /// Compares constraint *params* as well as names so that `@min(10)` → `@min(20)`
    /// is detected as a change (M3).
    fn diff_constraints(
        model_name: &str,
        field_name: &str,
        old_field: &SimpleField,
        new_field: &SimpleField,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // M2: BTreeMap for deterministic ordering
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

        // Removed constraints (sorted by name)
        for (constraint_name, _) in old_constraints.iter() {
            if !new_constraints.contains_key(constraint_name) {
                changes.push(SchemaChange::RemoveConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                });
            }
        }

        // Added or changed constraints (sorted by name)
        for (constraint_name, new_constraint) in new_constraints.iter() {
            if let Some(old_constraint) = old_constraints.get(constraint_name) {
                // M3: detect param changes (e.g. @min(10) → @min(20))
                if old_constraint.params != new_constraint.params {
                    // Emit a remove + add pair to represent the param update
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

    /// Diff composite indexes.
    ///
    /// Iterates in sorted order for deterministic change output (M2).
    fn diff_composite_indexes(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // Convert to HashSets for membership testing, but collect results in
        // sorted order (M2) by converting from a sorted BTreeSet-like iteration.
        let old_indexes: HashSet<_> = old_model.composite_indexes.iter().cloned().collect();
        let new_indexes: HashSet<_> = new_model.composite_indexes.iter().cloned().collect();

        // Removed indexes — sort for determinism
        let mut removed: Vec<_> = old_indexes.iter().filter(|idx| !new_indexes.contains(*idx)).collect();
        removed.sort();
        for old_idx in removed {
            changes.push(SchemaChange::RemoveCompositeIndex {
                model_name: model_name.to_string(),
                fields: old_idx.clone(),
            });
        }

        // Added indexes — sort for determinism
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
