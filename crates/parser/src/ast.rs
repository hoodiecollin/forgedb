use forgedb_validation::Position;

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintParam {
    Number(i64),
    Fractional(String),
    String(String),
    Named {
        name: String,
        value: Box<ConstraintParam>,
    },
    Exclusive {
        greater: bool,
        value: Box<ConstraintParam>,
    },
}

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
    Hash,
    BTree,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeIndex {
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub name: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimestampPrecision {
    Seconds,
    #[default]
    Millis,
    Micros,
}

impl TimestampPrecision {
    pub fn key(&self) -> &'static str {
        match self {
            TimestampPrecision::Seconds => "s",
            TimestampPrecision::Millis => "ms",
            TimestampPrecision::Micros => "us",
        }
    }

    pub fn quantum_micros(&self) -> i64 {
        match self {
            TimestampPrecision::Seconds => 1_000_000,
            TimestampPrecision::Millis => 1_000,
            TimestampPrecision::Micros => 1,
        }
    }

    pub fn unit_noun(&self) -> &'static str {
        match self {
            TimestampPrecision::Seconds => "second",
            TimestampPrecision::Millis => "millisecond",
            TimestampPrecision::Micros => "microsecond",
        }
    }

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
    Timestamp(TimestampPrecision),
    Json,
    Decimal,
    Enum(String),
    Bytes(usize),
    StringN { chars: u8, exact: bool },
    FixedArray(Box<FieldType>, usize),
    StructType(String),
    OptionalStructType(String),
    Nullable(Box<FieldType>),
    Relation(RelationType),
    Component(ComponentReference),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentProtocol {
    Tsx,
    Jsx,
    Api,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelationInclusion {
    None,
    All,
    Specific(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentReference {
    pub protocol: ComponentProtocol,
    pub path: String,
    pub relations: RelationInclusion,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelationType {
    OneToMany(String),
    RequiredReference(String),
    OptionalReference(String),
    ManyToMany(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool,
    pub unique: bool,
    pub indexed: bool,
    pub constraints: Vec<Constraint>,
    pub index_type: IndexType,
    pub is_computed: bool,
    pub fulltext_indexed: bool,
    pub is_materialized: bool,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<Field>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<String>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub composite_indexes: Vec<CompositeIndex>,
    pub projections: Vec<Projection>,
    pub soft_delete: bool,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub structs: Vec<Struct>,
    pub enums: Vec<EnumDef>,
    pub models: Vec<Model>,
}

impl Schema {
    pub fn find_model(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.name == name)
    }

    pub fn find_struct(&self, name: &str) -> Option<&Struct> {
        self.structs.iter().find(|s| s.name == name)
    }

    pub fn find_enum(&self, name: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|e| e.name == name)
    }

    pub fn detect_relations(&self) -> Vec<RelationPair> {
        let mut relations = Vec::new();

        for model in &self.models {
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(target_model)) =
                    &field.field_type
                {
                    if let Some(target) = self.find_model(target_model) {
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

    pub fn detect_many_to_many_relations(&self) -> Vec<ManyToManyRelation> {
        let mut m2m_relations = Vec::new();
        let mut processed_pairs = std::collections::HashSet::new();

        let one_to_many_field_pairs: std::collections::HashSet<(String, String, String, String)> =
            self.detect_relations()
                .iter()
                .map(|rel| {
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
                    if let Some(target) = self.find_model(target_model) {
                        for target_field in &target.fields {
                            if let FieldType::Relation(RelationType::OneToMany(back_ref)) =
                                &target_field.field_type
                            {
                                if back_ref == &model.name {
                                    let is_one_to_many = one_to_many_field_pairs.contains(&(
                                        model.name.clone(),
                                        field.name.clone(),
                                        target_model.clone(),
                                        target_field.name.clone(),
                                    ));

                                    if !is_one_to_many {
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

    pub fn is_identity_key(&self) -> bool {
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

    pub fn is_junction_key(&self) -> bool {
        self.is_identity_key()
    }

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
            | FieldType::Decimal
            | FieldType::Enum(_)
            | FieldType::Bytes(_) => true,
            FieldType::FixedArray(inner, _) => inner.is_fixed_size(),
            FieldType::StructType(_) => true,
            FieldType::OptionalStructType(_) => true,
            FieldType::Nullable(inner) => inner.is_fixed_size(),
            FieldType::String => false,
            FieldType::StringN { .. } => false,
            FieldType::Json => false,
            FieldType::Relation(_) => false,
            FieldType::Component(_) => false,
        }
    }

    pub fn struct_name(&self) -> Option<&str> {
        match self {
            FieldType::StructType(name) => Some(name),
            FieldType::OptionalStructType(name) => Some(name),
            _ => None,
        }
    }

    pub fn enum_name(&self) -> Option<&str> {
        match self {
            FieldType::Enum(name) => Some(name),
            FieldType::Nullable(inner) => inner.enum_name(),
            _ => None,
        }
    }

    pub fn size_in_bytes(&self, schema: &Schema) -> usize {
        match self {
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp(_) => 8,
            FieldType::Bool => 1,
            FieldType::Enum(_) => 1,
            FieldType::Uuid => 16,
            FieldType::Decimal => 16,
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
                1 + if let Some(struct_def) = schema.find_struct(name) {
                    Struct::calculate_size(struct_def, schema)
                } else {
                    0
                }
            }
            FieldType::Nullable(inner) => inner.size_in_bytes(schema),
            _ => 0,
        }
    }

    pub fn alignment(&self, schema: &Schema) -> usize {
        match self {
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::F64 | FieldType::Timestamp(_) => 8,
            FieldType::Bool => 1,
            FieldType::Enum(_) => 1,
            FieldType::Uuid => 16,
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

    pub fn default_index_type(&self) -> IndexType {
        if self.supports_range_queries() {
            IndexType::BTree
        } else {
            IndexType::Hash
        }
    }
}

impl Struct {
    pub fn calculate_size(struct_def: &Struct, schema: &Schema) -> usize {
        let mut size = 0;
        let mut max_alignment = 1;

        for field in &struct_def.fields {
            let field_size = field.field_type.size_in_bytes(schema);
            let field_align = field.field_type.alignment(schema);

            max_alignment = max_alignment.max(field_align);

            if size % field_align != 0 {
                size += field_align - (size % field_align);
            }

            size += field_size;
        }

        if size % max_alignment != 0 {
            size += max_alignment - (size % max_alignment);
        }

        size
    }

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
    pub fn identity_field(&self) -> Option<&Field> {
        self.fields
            .iter()
            .find(|f| f.name == "id")
            .or_else(|| self.fields.iter().find(|f| f.auto_generate))
    }

    pub fn has_identity(&self) -> bool {
        self.identity_field().is_some()
    }
}

impl Field {
    pub fn has_constraint(&self, name: &str) -> bool {
        self.constraints.iter().any(|c| c.name == name)
    }

    pub fn get_constraint(&self, name: &str) -> Option<&Constraint> {
        self.constraints.iter().find(|c| c.name == name)
    }

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
