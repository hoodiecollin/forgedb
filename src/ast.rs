/// Abstract Syntax Tree representation

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    U32,
    U64,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool, // + symbol
    pub unique: bool,        // & symbol
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
            FieldType::String => "String",
        }
    }
}
