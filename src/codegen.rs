use crate::ast::{Field, Model, Schema};

pub struct CodeGenerator;

impl CodeGenerator {
    pub fn new() -> Self {
        CodeGenerator
    }

    fn generate_field_declaration(&self, field: &Field) -> String {
        let rust_type = field.field_type.to_rust_type();
        format!("    pub {}: {},", field.name, rust_type)
    }

    fn generate_struct(&self, model: &Model) -> String {
        let mut code = String::new();

        // Generate the main struct
        code.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\n"));
        code.push_str(&format!("pub struct {} {{\n", model.name));

        for field in &model.fields {
            code.push_str(&self.generate_field_declaration(field));
            code.push('\n');
        }

        code.push_str("}\n\n");

        code
    }

    fn generate_storage_struct(&self, model: &Model) -> String {
        let mut code = String::new();

        code.push_str(&format!("pub struct {}Storage {{\n", model.name));
        code.push_str(&format!("    records: Vec<{}>,\n", model.name));
        code.push_str("    next_id: u64,\n");
        code.push_str("    tombstones: Vec<bool>,\n");

        // Add unique index maps for fields with & symbol
        for field in &model.fields {
            if field.unique {
                code.push_str(&format!("    {}_index: std::collections::HashMap<{}, usize>,\n",
                    field.name, field.field_type.to_rust_type()));
            }
        }

        code.push_str("}\n\n");

        code
    }

    fn generate_storage_impl(&self, model: &Model) -> String {
        let mut code = String::new();

        code.push_str(&format!("impl {}Storage {{\n", model.name));

        // Generate new() method
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str(&format!("        {}Storage {{\n", model.name));
        code.push_str("            records: Vec::new(),\n");
        code.push_str("            next_id: 1,\n");
        code.push_str("            tombstones: Vec::new(),\n");

        for field in &model.fields {
            if field.unique {
                code.push_str(&format!("            {}_index: std::collections::HashMap::new(),\n", field.name));
            }
        }

        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // Generate insert() method
        self.generate_insert_method(&mut code, model);

        // Generate get() method
        self.generate_get_method(&mut code, model);

        code.push_str("}\n\n");

        code
    }

    fn generate_insert_method(&self, code: &mut String, model: &Model) {
        // Find auto-generate field (assumed to be u64 id)
        let auto_gen_field = model.fields.iter().find(|f| f.auto_generate);

        // Find unique fields
        let unique_fields: Vec<&Field> = model.fields.iter().filter(|f| f.unique).collect();

        code.push_str("    pub fn insert(&mut self");

        // Parameters: all fields except auto-generated ones
        for field in &model.fields {
            if !field.auto_generate {
                code.push_str(&format!(", {}: {}", field.name, field.field_type.to_rust_type()));
            }
        }

        code.push_str(&format!(") -> Result<{}, String> {{\n", model.name));

        // Check unique constraints
        for field in &unique_fields {
            code.push_str(&format!("        if self.{}_index.contains_key(&{}) {{\n", field.name, field.name));
            code.push_str(&format!("            return Err(\"Unique constraint violation: {} already exists\".to_string());\n", field.name));
            code.push_str("        }\n");
        }

        // Generate auto-increment ID if needed
        if let Some(id_field) = auto_gen_field {
            code.push_str(&format!("        let {} = self.next_id;\n", id_field.name));
            code.push_str("        self.next_id += 1;\n");
        }

        // Create record
        code.push_str(&format!("        let record = {} {{\n", model.name));
        for field in &model.fields {
            code.push_str(&format!("            {},\n", field.name));
        }
        code.push_str("        };\n\n");

        // Add to unique indexes
        for field in &unique_fields {
            code.push_str(&format!("        self.{}_index.insert(record.{}.clone(), self.records.len());\n",
                field.name, field.name));
        }

        // Add record
        code.push_str("        self.records.push(record.clone());\n");
        code.push_str("        self.tombstones.push(false);\n");
        code.push_str("        Ok(record)\n");
        code.push_str("    }\n\n");
    }

    fn generate_get_method(&self, code: &mut String, model: &Model) {
        // Find the ID field (first field with auto_generate)
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!("    pub fn get(&self, {}: {}) -> Option<{}> {{\n",
                id_field.name, id_field.field_type.to_rust_type(), model.name));
            code.push_str("        self.records.iter().enumerate()\n");
            code.push_str("            .find(|(i, r)| !self.tombstones[*i] && r.");
            code.push_str(&format!("{} == {})\n", id_field.name, id_field.name));
            code.push_str("            .map(|(_, r)| r.clone())\n");
            code.push_str("    }\n");
        }
    }

    pub fn generate(&self, schema: &Schema) -> String {
        let mut code = String::new();

        // Add standard imports
        code.push_str("// Generated code - do not edit manually\n\n");
        code.push_str("use std::collections::HashMap;\n\n");

        // Generate code for each model
        for model in &schema.models {
            code.push_str(&self.generate_struct(model));
            code.push_str(&self.generate_storage_struct(model));
            code.push_str(&self.generate_storage_impl(model));
        }

        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn test_generate_simple_model() {
        let input = r#"
User {
  id: +u64
  email: &string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check that code contains expected elements
        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub id: u64"));
        assert!(code.contains("pub email: String"));
        assert!(code.contains("pub struct UserStorage"));
        assert!(code.contains("pub fn insert"));
        assert!(code.contains("pub fn get"));
    }

    #[test]
    fn test_generate_multiple_models() {
        let input = r#"
User {
  id: +u64
  email: &string
}

Post {
  id: +u64
  title: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check User model
        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub struct UserStorage"));

        // Check Post model
        assert!(code.contains("pub struct Post"));
        assert!(code.contains("pub struct PostStorage"));

        // Both should have their own methods
        assert!(code.matches("pub fn insert").count() >= 2);
        assert!(code.matches("pub fn get").count() >= 2);
    }

    #[test]
    fn test_generate_plain_field() {
        let input = r#"
User {
  id: +u64
  name: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Plain field should be in struct
        assert!(code.contains("pub name: String"));

        // Plain field should be a parameter in insert
        assert!(code.contains("name: String"));
    }

    #[test]
    fn test_generate_multiple_unique_fields() {
        let input = r#"
User {
  id: +u64
  email: &string
  username: &string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Should generate separate indexes for each unique field
        assert!(code.contains("email_index: std::collections::HashMap<String, usize>"));
        assert!(code.contains("username_index: std::collections::HashMap<String, usize>"));

        // Should check both unique constraints
        assert!(code.contains("if self.email_index.contains_key(&email)"));
        assert!(code.contains("if self.username_index.contains_key(&username)"));
    }

    #[test]
    fn test_generate_both_symbols() {
        let input = r#"
User {
  id: +&u64
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Should have unique index for id
        assert!(code.contains("id_index: std::collections::HashMap<u64, usize>"));

        // Should auto-generate id
        assert!(code.contains("let id = self.next_id"));

        // Should check unique constraint on id
        assert!(code.contains("if self.id_index.contains_key(&id)"));
    }

    #[test]
    fn test_generate_all_primitive_types() {
        let input = r#"
Model {
  field1: u32
  field2: u64
  field3: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        assert!(code.contains("pub field1: u32"));
        assert!(code.contains("pub field2: u64"));
        assert!(code.contains("pub field3: String"));
    }
}
