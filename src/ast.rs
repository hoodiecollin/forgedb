/// Abstract Syntax Tree representation

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool, // + symbol
    pub unique: bool,        // & symbol
    pub indexed: bool,       // ^ symbol
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

impl FieldType {
    pub fn to_rust_type(&self) -> &str {
        match self {
            FieldType::U32 => "u32",
            FieldType::U64 => "u64",
            FieldType::I32 => "i32",
            FieldType::I64 => "i64",
            FieldType::F64 => "f64",
            FieldType::Bool => "bool",
            FieldType::String => "String",
            FieldType::Uuid => "uuid::Uuid",
            FieldType::Timestamp => "i64",
        }
    }

    pub fn is_auto_incrementable(&self) -> bool {
        matches!(self, FieldType::U32 | FieldType::U64)
    }

    pub fn is_auto_generatable(&self) -> bool {
        matches!(self, FieldType::U32 | FieldType::U64 | FieldType::Uuid | FieldType::Timestamp)
    }
}
