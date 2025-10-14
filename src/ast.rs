/// Abstract Syntax Tree representation

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
    // Fixed-size types (Sprint 8)
    Char(usize),                       // Fixed-size character array: char(N)
    FixedArray(Box<FieldType>, usize), // Fixed array: [type; count]
    StructType(String),                // Reference to a struct by name
    OptionalStructType(String),        // Optional struct reference
    // Relations
    Relation(RelationType),
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
}

/// Represents a struct definition (Sprint 8)
#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub composite_indexes: Vec<CompositeIndex>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub structs: Vec<Struct>, // Struct definitions (Sprint 8)
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

    /// Validate all relations in the schema
    pub fn validate_relations(&self) -> Result<(), String> {
        for model in &self.models {
            for field in &model.fields {
                if let FieldType::Relation(rel_type) = &field.field_type {
                    let target = rel_type.target_model();
                    // Check if target model exists
                    if self.find_model(target).is_none() {
                        return Err(format!(
                            "Model '{}' field '{}' references undefined model '{}'",
                            model.name, field.name, target
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate struct references (Sprint 8)
    pub fn validate_struct_references(&self) -> Result<(), String> {
        // Validate structs don't contain variable-length types
        for struct_def in &self.structs {
            for field in &struct_def.fields {
                if !field.field_type.is_fixed_size() {
                    return Err(format!(
                        "Struct '{}' field '{}' contains variable-length type. Structs can only contain fixed-size types.",
                        struct_def.name, field.name
                    ));
                }
            }
        }

        // Validate struct references in models
        for model in &self.models {
            for field in &model.fields {
                if let Some(struct_name) = field.field_type.struct_name() {
                    if self.find_struct(struct_name).is_none() {
                        return Err(format!(
                            "Model '{}' field '{}' references undefined struct '{}'",
                            model.name, field.name, struct_name
                        ));
                    }
                }
            }
        }
        Ok(())
    }

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
            FieldType::Uuid => "uuid::Uuid".to_string(),
            FieldType::Timestamp => "i64".to_string(),
            FieldType::Char(size) => format!("[u8; {}]", size),
            FieldType::FixedArray(inner_type, count) => {
                format!("[{}; {}]", inner_type.to_rust_type(), count)
            }
            FieldType::StructType(name) => name.clone(),
            FieldType::OptionalStructType(name) => format!("Option<{}>", name),
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
            | FieldType::Char(_) => true,
            FieldType::FixedArray(inner, _) => inner.is_fixed_size(),
            FieldType::StructType(_) => true, // Structs must be fixed-size
            FieldType::OptionalStructType(_) => true, // Optional struct still fixed-size (uses discriminant)
            FieldType::String => false,
            FieldType::Relation(_) => false, // Relations are virtual or variable
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

    /// Get the size in bytes for fixed-size types (Sprint 8)
    pub fn size_in_bytes(&self, schema: &Schema) -> usize {
        match self {
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp => 8,
            FieldType::Bool => 1,
            FieldType::Uuid => 16,
            FieldType::Char(size) => *size,
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
            _ => 0, // Variable-size or virtual
        }
    }

    /// Get alignment requirement for this type (Sprint 8)
    pub fn alignment(&self, schema: &Schema) -> usize {
        match self {
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp => 8,
            FieldType::Bool => 1,
            FieldType::Uuid => 16, // UUID is typically 16-byte aligned
            FieldType::Char(_) => 1,
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

    /// Check if field is nullable (no constraint yet, but useful for future)
    pub fn is_nullable(&self) -> bool {
        // For now, only optional references and optional structs are nullable
        matches!(
            &self.field_type,
            FieldType::Relation(RelationType::OptionalReference(_))
                | FieldType::OptionalStructType(_)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_many_to_many() {
        // Create a simple schema with M:N relations
        let schema = Schema {
            structs: vec![],
            models: vec![
                Model {
                    name: "Post".to_string(),
                    fields: vec![
                        Field {
                            name: "id".to_string(),
                            field_type: FieldType::Uuid,
                            auto_generate: true,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                        Field {
                            name: "tags".to_string(),
                            field_type: FieldType::Relation(RelationType::OneToMany(
                                "Tag".to_string(),
                            )),
                            auto_generate: false,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                    ],
                    composite_indexes: vec![],
                },
                Model {
                    name: "Tag".to_string(),
                    fields: vec![
                        Field {
                            name: "id".to_string(),
                            field_type: FieldType::Uuid,
                            auto_generate: true,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                        Field {
                            name: "posts".to_string(),
                            field_type: FieldType::Relation(RelationType::OneToMany(
                                "Post".to_string(),
                            )),
                            auto_generate: false,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                    ],
                    composite_indexes: vec![],
                },
            ],
        };

        let m2m = schema.detect_many_to_many_relations();
        assert_eq!(m2m.len(), 1);
        assert_eq!(m2m[0].model1, "Post");
        assert_eq!(m2m[0].field1, "tags");
        assert_eq!(m2m[0].model2, "Tag");
        assert_eq!(m2m[0].field2, "posts");
    }

    #[test]
    fn test_no_m2m_with_fk() {
        // Schema with 1:N relationship (should not be detected as M:N)
        let schema = Schema {
            structs: vec![],
            models: vec![
                Model {
                    name: "User".to_string(),
                    fields: vec![
                        Field {
                            name: "id".to_string(),
                            field_type: FieldType::Uuid,
                            auto_generate: true,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                        Field {
                            name: "posts".to_string(),
                            field_type: FieldType::Relation(RelationType::OneToMany(
                                "Post".to_string(),
                            )),
                            auto_generate: false,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                    ],
                    composite_indexes: vec![],
                },
                Model {
                    name: "Post".to_string(),
                    fields: vec![
                        Field {
                            name: "id".to_string(),
                            field_type: FieldType::Uuid,
                            auto_generate: true,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                        Field {
                            name: "author".to_string(),
                            field_type: FieldType::Relation(RelationType::RequiredReference(
                                "User".to_string(),
                            )),
                            auto_generate: false,
                            unique: false,
                            indexed: false,
                            constraints: vec![],
                            index_type: IndexType::Hash,
                            is_computed: false,
                            fulltext_indexed: false,
                        },
                    ],
                    composite_indexes: vec![],
                },
            ],
        };

        // Should not detect M:N because User.posts has a corresponding FK in Post.author
        let m2m = schema.detect_many_to_many_relations();
        assert_eq!(m2m.len(), 0);
    }
}
