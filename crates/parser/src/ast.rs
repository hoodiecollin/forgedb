/// Abstract Syntax Tree representation

use forgedb_validation::Position;

/// Constraint parameter value
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintParam {
    Number(i64),
    String(String),
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
    Timestamp,
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
            FieldType::String => "String".to_string(),
            FieldType::Json => "serde_json::Value".to_string(),
            FieldType::Decimal => "rust_decimal::Decimal".to_string(),
            FieldType::Enum(name) => name.clone(),
            FieldType::Uuid => "uuid::Uuid".to_string(),
            FieldType::Timestamp => "i64".to_string(),
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
            FieldType::U32 | FieldType::U64 | FieldType::Uuid | FieldType::Timestamp
        )
    }

    pub fn is_relation(&self) -> bool {
        matches!(self, FieldType::Relation(_))
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
            | FieldType::Timestamp
            | FieldType::Decimal // exact decimal is a fixed 16-byte column, like Uuid
            | FieldType::Enum(_) // enum is a fixed 1-byte discriminant column
            | FieldType::Bytes(_) => true,
            FieldType::FixedArray(inner, _) => inner.is_fixed_size(),
            FieldType::StructType(_) => true, // Structs must be fixed-size
            FieldType::OptionalStructType(_) => true, // Optional struct still fixed-size (uses discriminant)
            FieldType::Nullable(inner) => inner.is_fixed_size(),
            FieldType::String => false,
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
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp => 8,
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
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp => 8,
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

    /// Determine if this type supports range queries (ordered)
    pub fn supports_range_queries(&self) -> bool {
        matches!(
            self,
            FieldType::U32
                | FieldType::U64
                | FieldType::I32
                | FieldType::I64
                | FieldType::F64
                | FieldType::Timestamp
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
