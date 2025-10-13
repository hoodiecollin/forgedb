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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool, // + symbol
    pub unique: bool,        // & symbol
    pub indexed: bool,       // ^ symbol
    pub constraints: Vec<Constraint>, // @ directives
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub models: Vec<Model>,
}

impl Schema {
    /// Find a model by name
    pub fn find_model(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.name == name)
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

    /// Detect one-to-many relationships by finding matching reference and collection fields
    pub fn detect_relations(&self) -> Vec<RelationPair> {
        let mut relations = Vec::new();

        for model in &self.models {
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(target_model)) = &field.field_type {
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
                                        is_required: matches!(rel, RelationType::RequiredReference(_)),
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationPair {
    pub parent_model: String,
    pub parent_field: String,
    pub child_model: String,
    pub child_field: String,
    pub is_required: bool,
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
            FieldType::Relation(rel) => match rel {
                RelationType::RequiredReference(model) => format!("uuid::Uuid /* FK to {} */", model),
                RelationType::OptionalReference(model) => format!("Option<uuid::Uuid> /* FK to {} */", model),
                RelationType::OneToMany(_) => "/* virtual field - no storage */".to_string(),
            },
        }
    }

    pub fn is_auto_incrementable(&self) -> bool {
        matches!(self, FieldType::U32 | FieldType::U64)
    }

    pub fn is_auto_generatable(&self) -> bool {
        matches!(self, FieldType::U32 | FieldType::U64 | FieldType::Uuid | FieldType::Timestamp)
    }

    pub fn is_relation(&self) -> bool {
        matches!(self, FieldType::Relation(_))
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
        // For now, only optional references are nullable
        matches!(&self.field_type, FieldType::Relation(RelationType::OptionalReference(_)))
    }
}

impl RelationType {
    pub fn target_model(&self) -> &str {
        match self {
            RelationType::OneToMany(model) => model,
            RelationType::RequiredReference(model) => model,
            RelationType::OptionalReference(model) => model,
        }
    }

    pub fn is_one_to_many(&self) -> bool {
        matches!(self, RelationType::OneToMany(_))
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, RelationType::RequiredReference(_) | RelationType::OptionalReference(_))
    }
}
