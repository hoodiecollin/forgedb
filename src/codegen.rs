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

        // Add index maps for fields with ^ or & symbols
        for field in &model.fields {
            if field.unique {
                // Unique index (& symbol) - maps value to single row index
                code.push_str(&format!("    {}_index: std::collections::HashMap<{}, usize>,\n",
                    field.name, field.field_type.to_rust_type()));
            } else if field.indexed {
                // Non-unique index (^ symbol) - maps value to multiple row indices
                code.push_str(&format!("    {}_index: std::collections::HashMap<{}, Vec<usize>>,\n",
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
            if field.unique || field.indexed {
                code.push_str(&format!("            {}_index: std::collections::HashMap::new(),\n", field.name));
            }
        }

        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // Generate insert() method
        self.generate_insert_method(&mut code, model);

        // Generate get() method
        self.generate_get_method(&mut code, model);

        // Generate find_by_X methods for indexed fields
        self.generate_find_by_methods(&mut code, model);

        // Generate list() method
        self.generate_list_method(&mut code, model);

        // Generate update() method
        self.generate_update_method(&mut code, model);

        // Generate delete() method
        self.generate_delete_method(&mut code, model);

        code.push_str("}\n\n");

        code
    }

    fn generate_insert_method(&self, code: &mut String, model: &Model) {
        use crate::ast::FieldType;

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

        // Generate auto-generated fields
        for field in &model.fields {
            if field.auto_generate {
                match field.field_type {
                    FieldType::U32 | FieldType::U64 => {
                        code.push_str(&format!("        let {} = self.next_id;\n", field.name));
                        code.push_str("        self.next_id += 1;\n");
                    }
                    FieldType::Uuid => {
                        code.push_str(&format!("        let {} = Uuid::new_v4();\n", field.name));
                    }
                    FieldType::Timestamp => {
                        code.push_str(&format!("        let {} = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;\n", field.name));
                    }
                    _ => {}
                }
            }
        }

        // Create record
        code.push_str(&format!("        let record = {} {{\n", model.name));
        for field in &model.fields {
            code.push_str(&format!("            {},\n", field.name));
        }
        code.push_str("        };\n\n");

        // Add to indexes
        code.push_str("        let row_index = self.records.len();\n");
        for field in &model.fields {
            if field.unique {
                // Unique index: map value to single row index
                code.push_str(&format!("        self.{}_index.insert(record.{}.clone(), row_index);\n",
                    field.name, field.name));
            } else if field.indexed {
                // Non-unique index: append row index to list of indices
                code.push_str(&format!("        self.{}_index.entry(record.{}.clone()).or_insert_with(Vec::new).push(row_index);\n",
                    field.name, field.name));
            }
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
            code.push_str("    }\n\n");
        }
    }

    fn generate_find_by_methods(&self, code: &mut String, model: &Model) {
        // Generate find_by_X for indexed or unique fields
        for field in &model.fields {
            if field.indexed || field.unique {
                let method_name = format!("find_by_{}", field.name);
                code.push_str(&format!("    pub fn {}(&self, {}: {}) -> Vec<{}> {{\n",
                    method_name, field.name, field.field_type.to_rust_type(), model.name));

                if field.unique {
                    // Unique index: O(1) lookup, returns 0 or 1 results
                    code.push_str(&format!("        if let Some(&idx) = self.{}_index.get(&{}) {{\n", field.name, field.name));
                    code.push_str("            if !self.tombstones[idx] {\n");
                    code.push_str("                return vec![self.records[idx].clone()];\n");
                    code.push_str("            }\n");
                    code.push_str("        }\n");
                    code.push_str("        Vec::new()\n");
                } else {
                    // Non-unique index: O(1) lookup, may return multiple results
                    code.push_str(&format!("        if let Some(indices) = self.{}_index.get(&{}) {{\n", field.name, field.name));
                    code.push_str("            return indices.iter()\n");
                    code.push_str("                .filter(|&&idx| !self.tombstones[idx])\n");
                    code.push_str("                .map(|&idx| self.records[idx].clone())\n");
                    code.push_str("                .collect();\n");
                    code.push_str("        }\n");
                    code.push_str("        Vec::new()\n");
                }

                code.push_str("    }\n\n");
            }
        }
    }

    fn generate_list_method(&self, code: &mut String, model: &Model) {
        code.push_str(&format!("    pub fn list(&self) -> Vec<{}> {{\n", model.name));
        code.push_str("        self.records.iter().enumerate()\n");
        code.push_str("            .filter(|(i, _)| !self.tombstones[*i])\n");
        code.push_str("            .map(|(_, r)| r.clone())\n");
        code.push_str("            .collect()\n");
        code.push_str("    }\n\n");
    }

    fn generate_update_method(&self, code: &mut String, model: &Model) {
        // Find the ID field
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!("    pub fn update(&mut self, {}: {}", id_field.name, id_field.field_type.to_rust_type()));

            // Parameters: all non-auto-generated fields
            for field in &model.fields {
                if !field.auto_generate {
                    code.push_str(&format!(", {}: {}", field.name, field.field_type.to_rust_type()));
                }
            }

            code.push_str(&format!(") -> Result<{}, String> {{\n", model.name));

            // Find the record
            code.push_str("        let idx = self.records.iter().enumerate()\n");
            code.push_str(&format!("            .find(|(i, r)| !self.tombstones[*i] && r.{} == {})\n", id_field.name, id_field.name));
            code.push_str("            .map(|(i, _)| i)\n");
            code.push_str("            .ok_or_else(|| \"Record not found\".to_string())?;\n\n");

            // Remove old values from indexes
            for field in &model.fields {
                if field.unique {
                    code.push_str(&format!("        self.{}_index.remove(&self.records[idx].{});\n", field.name, field.name));
                } else if field.indexed {
                    code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", field.name, field.name));
                    code.push_str("            indices.retain(|&i| i != idx);\n");
                    code.push_str("        }\n");
                }
            }

            // Check unique constraints for new values
            for field in &model.fields {
                if field.unique && !field.auto_generate {
                    code.push_str(&format!("        if self.{}_index.contains_key(&{}) && self.records[idx].{} != {} {{\n",
                        field.name, field.name, field.name, field.name));
                    code.push_str(&format!("            return Err(\"Unique constraint violation: {} already exists\".to_string());\n", field.name));
                    code.push_str("        }\n");
                }
            }

            // Update record
            code.push_str(&format!("        self.records[idx] = {} {{\n", model.name));
            code.push_str(&format!("            {}: self.records[idx].{}.clone(),\n", id_field.name, id_field.name));
            for field in &model.fields {
                if !field.auto_generate {
                    code.push_str(&format!("            {},\n", field.name));
                }
            }
            code.push_str("        };\n\n");

            // Add new values to indexes
            for field in &model.fields {
                if field.unique {
                    code.push_str(&format!("        self.{}_index.insert(self.records[idx].{}.clone(), idx);\n", field.name, field.name));
                } else if field.indexed {
                    code.push_str(&format!("        self.{}_index.entry(self.records[idx].{}.clone()).or_insert_with(Vec::new).push(idx);\n",
                        field.name, field.name));
                }
            }

            code.push_str("        Ok(self.records[idx].clone())\n");
            code.push_str("    }\n\n");
        }
    }

    fn generate_delete_method(&self, code: &mut String, model: &Model) {
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!("    pub fn delete(&mut self, {}: {}) -> Result<(), String> {{\n",
                id_field.name, id_field.field_type.to_rust_type()));

            // Find the record
            code.push_str("        let idx = self.records.iter().enumerate()\n");
            code.push_str(&format!("            .find(|(i, r)| !self.tombstones[*i] && r.{} == {})\n", id_field.name, id_field.name));
            code.push_str("            .map(|(i, _)| i)\n");
            code.push_str("            .ok_or_else(|| \"Record not found\".to_string())?;\n\n");

            // Mark as deleted (tombstone)
            code.push_str("        self.tombstones[idx] = true;\n\n");

            // Remove from indexes (optional optimization to free memory)
            for field in &model.fields {
                if field.unique {
                    code.push_str(&format!("        self.{}_index.remove(&self.records[idx].{});\n", field.name, field.name));
                } else if field.indexed {
                    code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", field.name, field.name));
                    code.push_str("            indices.retain(|&i| i != idx);\n");
                    code.push_str("        }\n");
                }
            }

            code.push_str("        Ok(())\n");
            code.push_str("    }\n\n");
        }
    }

    pub fn generate(&self, schema: &Schema) -> String {
        let mut code = String::new();

        // Add standard imports
        code.push_str("// Generated code - do not edit manually\n\n");
        code.push_str("use std::collections::HashMap;\n");
        code.push_str("use std::time::{SystemTime, UNIX_EPOCH};\n");
        code.push_str("use uuid::Uuid;\n\n");

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
  field3: i32
  field4: i64
  field5: f64
  field6: bool
  field7: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        assert!(code.contains("pub field1: u32"));
        assert!(code.contains("pub field2: u64"));
        assert!(code.contains("pub field3: i32"));
        assert!(code.contains("pub field4: i64"));
        assert!(code.contains("pub field5: f64"));
        assert!(code.contains("pub field6: bool"));
        assert!(code.contains("pub field7: String"));
    }

    #[test]
    fn test_generate_uuid_type() {
        let input = r#"
User {
  id: +uuid
  email: &string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check UUID type is used
        assert!(code.contains("pub id: uuid::Uuid"));

        // Check UUID generation
        assert!(code.contains("let id = Uuid::new_v4()"));

        // Check uuid import
        assert!(code.contains("use uuid::Uuid"));
    }

    #[test]
    fn test_generate_timestamp_type() {
        let input = r#"
User {
  id: +u64
  created_at: +timestamp
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check timestamp type (stored as i64)
        assert!(code.contains("pub created_at: i64"));

        // Check timestamp generation
        assert!(code.contains("let created_at = SystemTime::now()"));
        assert!(code.contains("UNIX_EPOCH"));

        // Check imports
        assert!(code.contains("use std::time::{SystemTime, UNIX_EPOCH}"));
    }

    #[test]
    fn test_generate_full_type_schema() {
        let input = r#"
User {
  id: +uuid
  email: &string
  age: u32
  balance: f64
  active: bool
  score: i32
  created_at: +timestamp
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check all types are present
        assert!(code.contains("pub id: uuid::Uuid"));
        assert!(code.contains("pub email: String"));
        assert!(code.contains("pub age: u32"));
        assert!(code.contains("pub balance: f64"));
        assert!(code.contains("pub active: bool"));
        assert!(code.contains("pub score: i32"));
        assert!(code.contains("pub created_at: i64"));

        // Check auto-generation
        assert!(code.contains("let id = Uuid::new_v4()"));
        assert!(code.contains("let created_at = SystemTime::now()"));

        // Check email is a parameter (not auto-generated)
        assert!(code.contains("email: String"));
    }

    // Sprint 3: Indexing and Query Operations tests
    #[test]
    fn test_generate_indexed_field() {
        let input = r#"
User {
  id: +u64
  username: ^string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check non-unique index is generated (Vec<usize>)
        assert!(code.contains("username_index: std::collections::HashMap<String, Vec<usize>>"));

        // Check index is initialized
        assert!(code.contains("username_index: std::collections::HashMap::new()"));

        // Check index is maintained in insert
        assert!(code.contains("self.username_index.entry(record.username.clone()).or_insert_with(Vec::new).push(row_index)"));

        // Check find_by method is generated
        assert!(code.contains("pub fn find_by_username"));
    }

    #[test]
    fn test_generate_unique_indexed_field() {
        let input = r#"
User {
  id: +u64
  email: ^&string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check unique index is generated (usize, not Vec<usize>)
        assert!(code.contains("email_index: std::collections::HashMap<String, usize>"));

        // Check find_by method is generated
        assert!(code.contains("pub fn find_by_email"));
    }

    #[test]
    fn test_generate_list_method() {
        let input = r#"
User {
  id: +u64
  email: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check list method is generated
        assert!(code.contains("pub fn list(&self) -> Vec<User>"));

        // Check tombstone filtering
        assert!(code.contains("filter(|(i, _)| !self.tombstones[*i])"));
    }

    #[test]
    fn test_generate_update_method() {
        let input = r#"
User {
  id: +uuid
  email: string
  age: u32
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check update method is generated
        assert!(code.contains("pub fn update(&mut self, id: uuid::Uuid, email: String, age: u32)"));
        assert!(code.contains("-> Result<User, String>"));

        // Check record not found error
        assert!(code.contains("Record not found"));
    }

    #[test]
    fn test_generate_delete_method() {
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

        // Check delete method is generated
        assert!(code.contains("pub fn delete(&mut self, id: u64) -> Result<(), String>"));

        // Check tombstone marking
        assert!(code.contains("self.tombstones[idx] = true"));

        // Check index cleanup
        assert!(code.contains("self.email_index.remove(&self.records[idx].email)"));
    }

    #[test]
    fn test_generate_update_with_indexes() {
        let input = r#"
User {
  id: +uuid
  email: ^&string
  username: ^string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check update removes old values from indexes
        assert!(code.contains("self.email_index.remove(&self.records[idx].email)"));
        assert!(code.contains("self.username_index.get_mut(&self.records[idx].username)"));

        // Check update adds new values to indexes
        assert!(code.contains("self.email_index.insert(self.records[idx].email.clone(), idx)"));
        assert!(code.contains("self.username_index.entry(self.records[idx].username.clone()).or_insert_with(Vec::new).push(idx)"));

        // Check unique constraint validation on update
        assert!(code.contains("if self.email_index.contains_key(&email) && self.records[idx].email != email"));
    }
}
