/// Abstract Syntax Tree representation

use forgedb_validation::Position;

/// Constraint parameter value
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintParam {
    Number(i64),
    /// A fractional numeric literal, carried as its **verbatim source lexeme**
    /// (e.g. `"0.01"`, `"-273.15"`) — #239.
    ///
    /// Not an `f64`. Rounding into binary float here would happen before the
    /// target field type is known, which is inherent for an `f64` field but
    /// defeats the entire point of `decimal` — the type that exists precisely
    /// because `0.01` has no exact binary representation. Consumers convert
    /// against the known type: exact `Decimal` for a `decimal` field, correctly
    /// rounded `f64` for an `f64` field, and an error on an integer field.
    Fractional(String),
    String(String),
    /// A named argument, `name: value` (#235) — as in `@length(min: 3, max: 64)`.
    ///
    /// The value is boxed rather than fixed to `i64` so a future value kind (a
    /// float, for the fractional numeric bounds of #239) composes here without a
    /// second grammar pass over the parameter loop.
    Named {
        name: String,
        value: Box<ConstraintParam>,
    },
    /// An exclusive bound written with a comparison operator — `@min(>0)` /
    /// `@max(<1)` (#239).
    ///
    /// Only meaningful on a continuous domain: on integers `>0` and `>=1` denote
    /// the same set, so validation rejects the operator form there rather than
    /// admitting a second spelling that buys no expressiveness.
    ///
    /// The operator is recorded rather than collapsed to "exclusive" so a
    /// nonsensical pairing (`@min(<5)`) is rejected instead of being silently
    /// read as an exclusive minimum.
    Exclusive {
        /// `true` for `>`, `false` for `<`.
        greater: bool,
        value: Box<ConstraintParam>,
    },
}

/// Constraint directive (e.g., @min(10), @email)
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub name: String,
    pub params: Vec<ConstraintParam>,
}

impl Constraint {
    pub fn new(name: String) -> Self {
        Constraint {
            name,
            params: Vec::new(),
        }
    }

    pub fn with_param(mut self, param: ConstraintParam) -> Self {
        self.params.push(param);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexType {
    Hash,  // Default for exact matches, unordered types
    BTree, // For range queries on ordered types
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeIndex {
    pub fields: Vec<String>,
}

/// A model-level `@projection(<name>: <field>, ...)` directive (#113): a named,
/// compile-time-known subset of a model's stored columns.  Generates a tailored
/// projection struct + narrow read that materializes only PK + these fields.
/// The identity column is always materialized regardless of whether it appears
/// in `fields` (validation/codegen enforce this).
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub name: String,
    pub fields: Vec<String>,
}

/// The declared precision of a `timestamp` field (#254).
///
/// It is NOT the storage unit — every timestamp persists as `i64` microseconds,
/// because a per-field unit would make a `Timestamp` value unit-ambiguous at
/// runtime and cost either a fatter value (a layout change) or a
/// `Timestamp<const U>` (generated signature churn everywhere).
///
/// What it governs is the **quantum**: a user-supplied value is floored to it on
/// write, and an allocated `+timestamp` identity advances by one unit of it. So
/// it buys semantic *fidelity*, not correctness — under a burst of N rows in one
/// tick the allocator runs N units ahead of the wall clock, and recovery time is
/// proportional to the unit. That is the whole argument for the `us` floor on an
/// allocated identity: the same million-row import that lands rows ~17 minutes
/// in the future at `ms` lands them 1 second ahead at `us`.
///
/// `ns` is not offerable: the keys are bounded by the microsecond storage unit,
/// and `i64` nanoseconds would cap the type at 1678–2262.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimestampPrecision {
    /// `timestamp(s)` — whole seconds.
    Seconds,
    /// `timestamp(ms)` — milliseconds. **The default**, so a bare `timestamp`
    /// is `timestamp(ms)`: making it a property of the type rather than a rule
    /// the parser has to remember means no site can disagree about it.
    #[default]
    Millis,
    /// `timestamp(us)` — microseconds, the storage unit itself. The only
    /// precision an allocated `+timestamp` identity may declare.
    Micros,
}

impl TimestampPrecision {
    /// The spelling that parses back to this precision.
    pub fn key(&self) -> &'static str {
        match self {
            TimestampPrecision::Seconds => "s",
            TimestampPrecision::Millis => "ms",
            TimestampPrecision::Micros => "us",
        }
    }

    /// This precision's quantum, in microseconds — what a written value is
    /// floored to, and what an allocated identity advances by.
    pub fn quantum_micros(&self) -> i64 {
        match self {
            TimestampPrecision::Seconds => 1_000_000,
            TimestampPrecision::Millis => 1_000,
            TimestampPrecision::Micros => 1,
        }
    }

    /// The singular English noun for this quantum, for diagnostics that have to
    /// read as a sentence (`"one millisecond at a time"`) rather than as a key.
    pub fn unit_noun(&self) -> &'static str {
        match self {
            TimestampPrecision::Seconds => "second",
            TimestampPrecision::Millis => "millisecond",
            TimestampPrecision::Micros => "microsecond",
        }
    }

    /// Parse a declared precision key. `None` for anything else, including `ns`
    /// (bounded out by the microsecond storage unit).
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "s" => Some(TimestampPrecision::Seconds),
            "ms" => Some(TimestampPrecision::Millis),
            "us" => Some(TimestampPrecision::Micros),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    U32,
    U64,
    I32,
    I64,
    F64,
    Bool,
    String,
    Uuid,
    /// An instant, `timestamp` or `timestamp(s|ms|us)` (#254).
    ///
    /// The declared precision is carried **in the variant** rather than hung off
    /// [`Field`], for two reasons that are not stylistic: an array's inner type
    /// is a `FieldType` (so `[timestamp(us); 4]` is otherwise inexpressible), and
    /// [`FieldType::Nullable`] wraps a `FieldType` (so `timestamp(us)?` would
    /// otherwise lose its precision). Carrying it here also makes the sweep a
    /// compiler-enforced audit: every site that matched a bare `Timestamp` has to
    /// say what it does about precision, and the failure mode this issue guards
    /// against is silent.
    ///
    /// Storage is always microseconds regardless of the declared key; the
    /// precision is the **allocation quantum** and the guarantee about what is
    /// stored, not a second on-disk unit. See [`TimestampPrecision`].
    Timestamp(TimestampPrecision),
    /// JSON value stored as its serialized bytes in a variable-length column,
    /// typed `serde_json::Value` in generated Rust (#json). Rides the same
    /// variable-column storage path as `String`.
    Json,
    /// Exact fixed-point decimal (money/quantity). Typed `rust_decimal::Decimal`
    /// in generated Rust; stored in a FIXED 16-byte column (via `Decimal::serialize`),
    /// exactly like `Uuid`. Serialized as a JSON string to preserve precision.
    /// `Ord`+`Hash`, so it is filterable/sortable/indexable (index key normalized
    /// to be scale-invariant). Bare `decimal` only — `decimal(p, s)` is deferred.
    Decimal,
    /// A reference to a user-declared top-level `enum Name { ... }` by its bare
    /// PascalCase name (#enum). Typed as the generated Rust enum in `database.rs`,
    /// serialized as the variant NAME string (so REST/TS/JSON all agree). Stored in
    /// a FIXED 1-byte `u8` discriminant column (variants map to `0..N` in declaration
    /// order). `Ord`+`Hash`, so it is filterable/sortable/indexable (index key = the
    /// variant name string). A bare PascalCase identifier that is NOT a declared enum
    /// stays a `StructType` and is caught by struct-reference validation.
    Enum(String),
    // Fixed-size types (Sprint 8)
    /// Fixed-size **byte** array: `bytes(N)` → `[u8; N]`.
    ///
    /// There is no UTF-8 guarantee, no length tracking, and no text semantics
    /// anywhere in the pipeline — on the wire it is an array of integers. The
    /// deprecated spelling `char(N)` (#233) parses to this same variant and warns;
    /// `char` was a false friend, since SQL's `CHAR(N)` is fixed-length *text*.
    Bytes(usize),
    /// `string(N)` / `string(N!)` — a fixed-width **inline string** column (#238).
    ///
    /// The value lives in the row's own slot of a `FixedColumn` rather than
    /// behind an `(offset, length)` pair in a `VariableColumn`, which is what
    /// makes a scan of it read one contiguous run instead of chasing pointers.
    /// Bare [`FieldType::String`] is untouched and still variable-width.
    ///
    /// `chars` is N, the **character** count (res 3) — not a byte count, and
    /// consistent with `@length`, which also counts characters. One byte per
    /// character by default, four under `@utf8` (res 4/5); the physical slot
    /// width is therefore a function of the declaration *and* that directive, and
    /// is computed in codegen, never here.
    ///
    /// `chars` is a `u8` rather than a `usize` on purpose: res 7 caps N at 255,
    /// and carrying the cap in the type makes an over-wide N unrepresentable
    /// instead of merely rejected. The check happens exactly once, where the
    /// parser reads the literal — there is no downstream site that has to
    /// remember it.
    ///
    /// `exact` is the `!` (res 2): at-most-N when `false`, exactly-N when `true`.
    /// The exact form is the narrow one — every value is exactly N characters, so
    /// under the default alphabet it is exactly N bytes and the slot carries no
    /// length prefix at all.
    ///
    /// There is no overflow. A value exceeding N is a write error (res 1);
    /// experiment #261 measured the inline-or-overflow alternative losing in 198
    /// of 200 configurations.
    StringN { chars: u8, exact: bool },
    FixedArray(Box<FieldType>, usize), // Fixed array: [type; count]
    StructType(String),                // Reference to a struct by name
    OptionalStructType(String),        // Optional struct reference
    /// Nullable wrapper for primitive/scalar types (e.g. `age: ?i32` or `age: i32?`)
    Nullable(Box<FieldType>),
    // Relations
    Relation(RelationType),
    // Component references (Sprint 17)
    Component(ComponentReference),
}

/// Component protocol (Sprint 17)
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentProtocol {
    Tsx,  // tsx://
    Jsx,  // jsx://
    Api,  // api://
}

/// Relation inclusion for components (Sprint 17)
#[derive(Debug, Clone, PartialEq)]
pub enum RelationInclusion {
    None,
    All,
    Specific(Vec<String>),
}

/// Component reference (Sprint 17)
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentReference {
    pub protocol: ComponentProtocol,
    pub path: String,
    pub relations: RelationInclusion,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelationType {
    /// One-to-many: [Post] means this model has many Posts
    OneToMany(String),
    /// Required reference: *User means this model must reference a User
    RequiredReference(String),
    /// Optional reference: ?User means this model optionally references a User
    OptionalReference(String),
    /// Many-to-many: detected from bidirectional OneToMany fields
    ManyToMany(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool,          // + symbol
    pub unique: bool,                 // & symbol
    pub indexed: bool,                // ^ symbol
    pub constraints: Vec<Constraint>, // @ directives
    pub index_type: IndexType,        // Hash or BTree
    pub is_computed: bool,            // @computed directive
    pub fulltext_indexed: bool,       // @fulltext directive (Sprint 18)
    pub is_materialized: bool,        // @materialized directive (Sprint 19)
    /// Source position of the field name (1-based line/column). `None` when the
    /// node is synthesized rather than parsed (e.g. test fixtures, migrations).
    pub position: Option<Position>,
}

/// Represents a struct definition (Sprint 8)
#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<Field>,
    /// Source position of the struct name (`None` when synthesized).
    pub position: Option<Position>,
}

/// A user-declared top-level `enum Name { V1, V2, ... }` (#enum).  A sibling of
/// `Model`/`Struct`.  Referenced from a field by its bare PascalCase name.
/// Variants are PascalCase, unique, non-empty, and map to `0..N` (the stored
/// `u8` discriminant) in declaration order.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<String>,
    /// Source position of the enum name (`None` when synthesized).
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub composite_indexes: Vec<CompositeIndex>,
    pub projections: Vec<Projection>, // @projection(name: a, b) directives (#113)
    pub soft_delete: bool,            // @soft_delete directive at model level (Sprint 19)
    /// Source position of the model name (`None` when synthesized).
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub structs: Vec<Struct>, // Struct definitions (Sprint 8)
    pub enums: Vec<EnumDef>,   // Enum definitions (#enum)
    pub models: Vec<Model>,
}

impl Schema {
    /// Find a model by name
    pub fn find_model(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.name == name)
    }

    /// Find a struct by name (Sprint 8)
    pub fn find_struct(&self, name: &str) -> Option<&Struct> {
        self.structs.iter().find(|s| s.name == name)
    }

    /// Find an enum by name (#enum)
    pub fn find_enum(&self, name: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|e| e.name == name)
    }

    // Schema-level semantic validation (relation targets, struct/enum references,
    // fixed-size struct fields, duplicate names, naming conventions) lives in
    // `crate::validate` — the single positioned authority consumed by the parser,
    // the CLI, and the LSP. See that module for the rationale on why it lives in
    // this crate rather than `forgedb-validation`.

    /// Detect one-to-many relationships by finding matching reference and collection fields
    pub fn detect_relations(&self) -> Vec<RelationPair> {
        let mut relations = Vec::new();

        for model in &self.models {
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(target_model)) =
                    &field.field_type
                {
                    // Found a one-to-many field, look for corresponding FK in target model
                    if let Some(target) = self.find_model(target_model) {
                        // Find a reference back to this model
                        for target_field in &target.fields {
                            if let FieldType::Relation(rel) = &target_field.field_type {
                                if rel.is_reference() && rel.target_model() == model.name {
                                    relations.push(RelationPair {
                                        parent_model: model.name.clone(),
                                        parent_field: field.name.clone(),
                                        child_model: target.name.clone(),
                                        child_field: target_field.name.clone(),
                                        is_required: matches!(
                                            rel,
                                            RelationType::RequiredReference(_)
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        relations
    }

    /// Detect many-to-many relationships by finding bidirectional OneToMany fields
    /// Returns pairs where Model A has [ModelB] and Model B has [ModelA]
    /// Excludes relationships that are already handled by FK references (1:N)
    pub fn detect_many_to_many_relations(&self) -> Vec<ManyToManyRelation> {
        let mut m2m_relations = Vec::new();
        let mut processed_pairs = std::collections::HashSet::new();

        // First, identify all 1:N relationships (OneToMany with matching FK)
        // We need to track specific FIELD pairs, not just model pairs
        let one_to_many_field_pairs: std::collections::HashSet<(String, String, String, String)> =
            self.detect_relations()
                .iter()
                .map(|rel| {
                    // For a 1:N relation, the parent's OneToMany field and child's FK field form a pair
                    // We store both orderings to match against
                    vec![
                        (
                            rel.parent_model.clone(),
                            rel.parent_field.clone(),
                            rel.child_model.clone(),
                            rel.child_field.clone(),
                        ),
                        (
                            rel.child_model.clone(),
                            rel.child_field.clone(),
                            rel.parent_model.clone(),
                            rel.parent_field.clone(),
                        ),
                    ]
                })
                .flatten()
                .collect();

        for model in &self.models {
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(target_model)) =
                    &field.field_type
                {
                    // Check if target model has a OneToMany field pointing back to this model
                    if let Some(target) = self.find_model(target_model) {
                        for target_field in &target.fields {
                            if let FieldType::Relation(RelationType::OneToMany(back_ref)) =
                                &target_field.field_type
                            {
                                if back_ref == &model.name {
                                    // Check if this specific FIELD pair is NOT a 1:N relationship (no FK)
                                    // by checking if this field pair is in the one_to_many_field_pairs set
                                    let is_one_to_many = one_to_many_field_pairs.contains(&(
                                        model.name.clone(),
                                        field.name.clone(),
                                        target_model.clone(),
                                        target_field.name.clone(),
                                    ));

                                    if !is_one_to_many {
                                        // Found a true M:N relationship
                                        // Create a consistent ordering to avoid duplicates
                                        let mut models_fields = vec![
                                            (model.name.as_str(), field.name.as_str()),
                                            (target.name.as_str(), target_field.name.as_str()),
                                        ];
                                        models_fields.sort();

                                        let pair_key = format!(
                                            "{}:{}:{}:{}",
                                            models_fields[0].0,
                                            models_fields[0].1,
                                            models_fields[1].0,
                                            models_fields[1].1
                                        );

                                        if !processed_pairs.contains(&pair_key) {
                                            processed_pairs.insert(pair_key);

                                            // Always store with consistent ordering
                                            let (model1, field1, model2, field2) =
                                                if model.name < target.name {
                                                    (
                                                        model.name.clone(),
                                                        field.name.clone(),
                                                        target.name.clone(),
                                                        target_field.name.clone(),
                                                    )
                                                } else {
                                                    (
                                                        target.name.clone(),
                                                        target_field.name.clone(),
                                                        model.name.clone(),
                                                        field.name.clone(),
                                                    )
                                                };

                                            m2m_relations.push(ManyToManyRelation {
                                                model1,
                                                field1,
                                                model2,
                                                field2,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        m2m_relations
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationPair {
    pub parent_model: String,
    pub parent_field: String,
    pub child_model: String,
    pub child_field: String,
    pub is_required: bool,
}

/// Represents a many-to-many relationship between two models
#[derive(Debug, Clone, PartialEq)]
pub struct ManyToManyRelation {
    pub model1: String,
    pub field1: String,
    pub model2: String,
    pub field2: String,
}

impl FieldType {
    pub fn to_rust_type(&self) -> String {
        match self {
            FieldType::U32 => "u32".to_string(),
            FieldType::U64 => "u64".to_string(),
            FieldType::I32 => "i32".to_string(),
            FieldType::I64 => "i64".to_string(),
            FieldType::F64 => "f64".to_string(),
            FieldType::Bool => "bool".to_string(),
            // #238: an inline `string(N)` presents as an ordinary `String` in the
            // generated struct and on every wire. Only its *storage* differs —
            // a fixed slot instead of an (offset, length) pair — and the scan path
            // borrows out of that slot rather than allocating.
            FieldType::String | FieldType::StringN { .. } => "String".to_string(),
            FieldType::Json => "serde_json::Value".to_string(),
            FieldType::Decimal => "rust_decimal::Decimal".to_string(),
            FieldType::Enum(name) => name.clone(),
            FieldType::Uuid => "uuid::Uuid".to_string(),
            FieldType::Timestamp(_) => "i64".to_string(),
            FieldType::Bytes(size) => format!("[u8; {}]", size),
            FieldType::FixedArray(inner_type, count) => {
                format!("[{}; {}]", inner_type.to_rust_type(), count)
            }
            FieldType::StructType(name) => name.clone(),
            FieldType::OptionalStructType(name) => format!("Option<{}>", name),
            FieldType::Nullable(inner) => format!("Option<{}>", inner.to_rust_type()),
            FieldType::Relation(rel) => match rel {
                RelationType::RequiredReference(model) => {
                    format!("uuid::Uuid /* FK to {} */", model)
                }
                RelationType::OptionalReference(model) => {
                    format!("Option<uuid::Uuid> /* FK to {} */", model)
                }
                RelationType::OneToMany(_) => "/* virtual field - no storage */".to_string(),
                RelationType::ManyToMany(_) => {
                    "/* virtual field - stored in junction table */".to_string()
                }
            },
            FieldType::Component(_) => "/* component reference - no storage */".to_string(),
        }
    }

    pub fn is_auto_incrementable(&self) -> bool {
        matches!(self, FieldType::U32 | FieldType::U64)
    }

    pub fn is_auto_generatable(&self) -> bool {
        matches!(
            self,
            FieldType::U32 | FieldType::U64 | FieldType::Uuid | FieldType::Timestamp(_)
        )
    }

    pub fn is_relation(&self) -> bool {
        matches!(self, FieldType::Relation(_))
    }

    /// May a model whose identity is this type be an endpoint of a many-to-many
    /// junction (#266)?
    ///
    /// The junction stores each endpoint's id in a fixed-width column, indexes it
    /// in a `HashMap`, and frames it in a fixed-width replication record — so the
    /// key must be fixed-width, hashable and totally equatable. That admits
    /// `uuid`, the four integer types, `timestamp`, and — since #252 backed it
    /// with a `Copy + Hash + Eq` `InlineStr<N>` — `string(N)` / `string(N!)`,
    /// which is every shape an identity is realistically written in (all four
    /// `+` autos among them).
    ///
    /// A **bare** `string` is outside it and always will be: it is variable-width,
    /// so it can occupy neither a `FixedColumn` slot nor a fixed-width frame.
    /// (#252 refuses it as an identity one step earlier, so a schema never
    /// reaches this predicate carrying one.)
    ///
    /// This lives on the AST rather than in the generator because BOTH sides need
    /// it and they cannot see each other: `forgedb-codegen`'s `valid_m2m` filters
    /// on it, and the parser's validator reports a schema outside it as an error.
    /// If those two ever disagree, a relation is silently dropped again — which is
    /// exactly the failure #266 exists to remove.
    pub fn is_junction_key(&self) -> bool {
        matches!(
            self,
            FieldType::Uuid
                | FieldType::U32
                | FieldType::U64
                | FieldType::I32
                | FieldType::I64
                | FieldType::Timestamp(_)
                | FieldType::StringN { .. }
        )
    }

    // #251 RED STUB — deliberately admits everything, so the allow-list scenarios
    // fail on their assertions rather than on a missing symbol.
    pub fn is_identity_key(&self) -> bool {
        let _ = self;
        true
    }

    /// Check if this type is fixed-size (Sprint 8)
    pub fn is_fixed_size(&self) -> bool {
        match self {
            FieldType::U32
            | FieldType::U64
            | FieldType::I32
            | FieldType::I64
            | FieldType::F64
            | FieldType::Bool
            | FieldType::Uuid
            | FieldType::Timestamp(_)
            | FieldType::Decimal // exact decimal is a fixed 16-byte column, like Uuid
            | FieldType::Enum(_) // enum is a fixed 1-byte discriminant column
            | FieldType::Bytes(_) => true,
            FieldType::FixedArray(inner, _) => inner.is_fixed_size(),
            FieldType::StructType(_) => true, // Structs must be fixed-size
            FieldType::OptionalStructType(_) => true, // Optional struct still fixed-size (uses discriminant)
            FieldType::Nullable(inner) => inner.is_fixed_size(),
            FieldType::String => false,
            // #238: `string(N)` occupies a fixed-width *column slot*, but this
            // predicate asks a different question — may the type be embedded in an
            // inline `struct` (and, transitively, a `[T; N]`)? Those are stored by
            // transmuting the Rust value's bytes, and the Rust value here is a
            // heap `String`; embedding one would persist a pointer. So: no. The
            // codegen-side `is_fixed_size_type`, which decides column layout, says
            // yes — the two predicates are deliberately different questions.
            FieldType::StringN { .. } => false,
            FieldType::Json => false, // JSON is a variable-length column, like String
            FieldType::Relation(_) => false, // Relations are virtual or variable
            FieldType::Component(_) => false, // Components are virtual
        }
    }

    /// Get struct name if this is a struct type (Sprint 8)
    pub fn struct_name(&self) -> Option<&str> {
        match self {
            FieldType::StructType(name) => Some(name),
            FieldType::OptionalStructType(name) => Some(name),
            _ => None,
        }
    }

    /// Get the enum name if this is (or wraps) an enum reference (#enum).
    pub fn enum_name(&self) -> Option<&str> {
        match self {
            FieldType::Enum(name) => Some(name),
            FieldType::Nullable(inner) => inner.enum_name(),
            _ => None,
        }
    }

    /// Get the size in bytes for fixed-size types (Sprint 8)
    pub fn size_in_bytes(&self, schema: &Schema) -> usize {
        match self {
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp(_) => 8,
            FieldType::Bool => 1,
            FieldType::Enum(_) => 1, // 1-byte u8 discriminant
            FieldType::Uuid => 16,
            FieldType::Decimal => 16, // exact decimal is a fixed 16-byte column, like Uuid (#189)
            FieldType::Bytes(size) => *size,
            FieldType::FixedArray(inner, count) => inner.size_in_bytes(schema) * count,
            FieldType::StructType(name) => {
                if let Some(struct_def) = schema.find_struct(name) {
                    Struct::calculate_size(struct_def, schema)
                } else {
                    0
                }
            }
            FieldType::OptionalStructType(name) => {
                // Option adds 1 byte discriminant + size of struct
                1 + if let Some(struct_def) = schema.find_struct(name) {
                    Struct::calculate_size(struct_def, schema)
                } else {
                    0
                }
            }
            // Nullable wraps the inner type in Option; use the inner size (discriminant handled by Rust)
            FieldType::Nullable(inner) => inner.size_in_bytes(schema),
            _ => 0, // Variable-size or virtual
        }
    }

    /// Get alignment requirement for this type (Sprint 8)
    pub fn alignment(&self, schema: &Schema) -> usize {
        match self {
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp(_) => 8,
            FieldType::Bool => 1,
            FieldType::Enum(_) => 1, // 1-byte u8 discriminant
            FieldType::Uuid => 16, // UUID is typically 16-byte aligned
            FieldType::Bytes(_) => 1,
            FieldType::FixedArray(inner, _) => inner.alignment(schema),
            FieldType::StructType(name) => {
                if let Some(struct_def) = schema.find_struct(name) {
                    Struct::calculate_alignment(struct_def, schema)
                } else {
                    1
                }
            }
            FieldType::OptionalStructType(name) => {
                if let Some(struct_def) = schema.find_struct(name) {
                    Struct::calculate_alignment(struct_def, schema)
                } else {
                    1
                }
            }
            FieldType::Nullable(inner) => inner.alignment(schema),
            _ => 1,
        }
    }

    /// Whether an indexed field of this type also gets an **ordered** index
    /// (#169) — i.e. a `find_by_<field>_range` method — rather than exact-match
    /// lookup only.
    ///
    /// This is not a free-standing opinion about the type: it must agree with
    /// `RustGenerator::ordered_key_type`, which is what actually decides whether
    /// the ordered `BTreeMap` is emitted. The answer here reaches users as
    /// migration prose ("Add BTree index on 'Product.price'") via
    /// `Field::index_type`, so a disagreement is a visible lie about what was
    /// generated. `f64` was listed here while codegen excluded it, and `decimal`
    /// was omitted while codegen included it — both directions wrong (#242).
    /// The two are now pinned together by a drift guard in the codegen crate.
    ///
    /// `f64` is included: it has no `Ord`, but the ordered index keys it by its
    /// total-order `u64` encoding (#242), so it does answer range queries.
    ///
    /// `Nullable` falls through to `false` on purpose: `ordered_key_type` returns
    /// `None` for any nullable field regardless of its inner type.
    pub fn supports_range_queries(&self) -> bool {
        matches!(
            self,
            FieldType::U32
                | FieldType::U64
                | FieldType::I32
                | FieldType::I64
                | FieldType::F64
                | FieldType::Timestamp(_)
                | FieldType::Decimal
        )
    }

    /// Get the default index type for this field type
    pub fn default_index_type(&self) -> IndexType {
        if self.supports_range_queries() {
            IndexType::BTree
        } else {
            IndexType::Hash
        }
    }
}

impl Struct {
    /// Calculate the total size of a struct with proper padding (Sprint 8)
    pub fn calculate_size(struct_def: &Struct, schema: &Schema) -> usize {
        let mut size = 0;
        let mut max_alignment = 1;

        for field in &struct_def.fields {
            let field_size = field.field_type.size_in_bytes(schema);
            let field_align = field.field_type.alignment(schema);

            max_alignment = max_alignment.max(field_align);

            // Add padding before field if needed
            if size % field_align != 0 {
                size += field_align - (size % field_align);
            }

            size += field_size;
        }

        // Add final padding to make struct size a multiple of its alignment
        if size % max_alignment != 0 {
            size += max_alignment - (size % max_alignment);
        }

        size
    }

    /// Calculate the alignment requirement for a struct (Sprint 8)
    pub fn calculate_alignment(struct_def: &Struct, schema: &Schema) -> usize {
        struct_def
            .fields
            .iter()
            .map(|f| f.field_type.alignment(schema))
            .max()
            .unwrap_or(1)
    }
}

impl Model {
    // #251 RED STUB — deliberately the OLD single-pass form, so the scenarios
    // fail on their assertions rather than on a missing symbol.
    pub fn identity_field(&self) -> Option<&Field> {
        self.fields
            .iter()
            .find(|f| f.name == "id" || f.auto_generate)
    }

    pub fn has_identity(&self) -> bool {
        self.identity_field().is_some()
    }
}

impl Field {
    /// Check if field has a specific constraint
    pub fn has_constraint(&self, name: &str) -> bool {
        self.constraints.iter().any(|c| c.name == name)
    }

    /// Get a constraint by name
    pub fn get_constraint(&self, name: &str) -> Option<&Constraint> {
        self.constraints.iter().find(|c| c.name == name)
    }

    /// Check if field is nullable (optional references, optional structs, and nullable primitives)
    pub fn is_nullable(&self) -> bool {
        matches!(
            &self.field_type,
            FieldType::Relation(RelationType::OptionalReference(_))
                | FieldType::OptionalStructType(_)
                | FieldType::Nullable(_)
        )
    }
}

impl RelationType {
    pub fn target_model(&self) -> &str {
        match self {
            RelationType::OneToMany(model) => model,
            RelationType::RequiredReference(model) => model,
            RelationType::OptionalReference(model) => model,
            RelationType::ManyToMany(model) => model,
        }
    }

    pub fn is_one_to_many(&self) -> bool {
        matches!(self, RelationType::OneToMany(_))
    }

    pub fn is_reference(&self) -> bool {
        matches!(
            self,
            RelationType::RequiredReference(_) | RelationType::OptionalReference(_)
        )
    }

    pub fn is_many_to_many(&self) -> bool {
        matches!(self, RelationType::ManyToMany(_))
    }
}
