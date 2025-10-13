use crate::ast::{Field, FieldType, Model, RelationType, Schema};

pub struct CodeGenerator;

impl CodeGenerator {
    pub fn new() -> Self {
        CodeGenerator
    }

    fn generate_field_declaration(&self, field: &Field) -> String {
        // Skip OneToMany fields - they are virtual and don't store data
        if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
            return String::new();
        }

        // For reference fields, generate the FK field name
        let (field_name, rust_type) = match &field.field_type {
            FieldType::Relation(RelationType::RequiredReference(_)) => {
                (format!("{}_id", field.name), "uuid::Uuid".to_string())
            }
            FieldType::Relation(RelationType::OptionalReference(_)) => {
                (format!("{}_id", field.name), "Option<uuid::Uuid>".to_string())
            }
            _ => (field.name.clone(), field.field_type.to_rust_type()),
        };

        format!("    pub {}: {},", field_name, rust_type)
    }

    fn generate_struct(&self, model: &Model) -> String {
        let mut code = String::new();

        // Generate the main struct
        code.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\n"));
        code.push_str(&format!("pub struct {} {{\n", model.name));

        for field in &model.fields {
            let field_decl = self.generate_field_declaration(field);
            if !field_decl.is_empty() {
                code.push_str(&field_decl);
                code.push('\n');
            }
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

        // Add index maps for fields with ^ or & symbols, and for FK fields
        for field in &model.fields {
            // Skip OneToMany fields
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue;
            }

            // Get the actual field name and type for storage
            let (field_name, field_type) = match &field.field_type {
                FieldType::Relation(RelationType::RequiredReference(_)) => {
                    (format!("{}_id", field.name), "uuid::Uuid".to_string())
                }
                FieldType::Relation(RelationType::OptionalReference(_)) => {
                    (format!("{}_id", field.name), "Option<uuid::Uuid>".to_string())
                }
                _ => (field.name.clone(), field.field_type.to_rust_type()),
            };

            // FK fields are automatically indexed (non-unique)
            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique {
                // Unique index (& symbol) - maps value to single row index
                code.push_str(&format!("    {}_index: std::collections::HashMap<{}, usize>,\n",
                    field_name, field_type));
            } else if field.indexed || is_fk {
                // Non-unique index (^ symbol or FK) - maps value to multiple row indices
                code.push_str(&format!("    {}_index: std::collections::HashMap<{}, Vec<usize>>,\n",
                    field_name, field_type));
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
            // Skip OneToMany fields
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue;
            }

            let field_name = match &field.field_type {
                FieldType::Relation(rel) if rel.is_reference() => format!("{}_id", field.name),
                _ => field.name.clone(),
            };

            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique || field.indexed || is_fk {
                code.push_str(&format!("            {}_index: std::collections::HashMap::new(),\n", field_name));
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

    fn get_field_param_name(&self, field: &Field) -> String {
        match &field.field_type {
            FieldType::Relation(rel) if rel.is_reference() => format!("{}_id", field.name),
            _ => field.name.clone(),
        }
    }

    fn get_field_param_type(&self, field: &Field) -> String {
        match &field.field_type {
            FieldType::Relation(RelationType::RequiredReference(_)) => "uuid::Uuid".to_string(),
            FieldType::Relation(RelationType::OptionalReference(_)) => "Option<uuid::Uuid>".to_string(),
            _ => field.field_type.to_rust_type(),
        }
    }

    fn generate_insert_method(&self, code: &mut String, model: &Model) {
        // Find unique fields
        let unique_fields: Vec<&Field> = model.fields.iter().filter(|f| f.unique).collect();

        code.push_str("    pub fn insert(&mut self");

        // Parameters: all fields except auto-generated and OneToMany ones
        for field in &model.fields {
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue; // Skip virtual fields
            }

            if !field.auto_generate {
                let param_name = self.get_field_param_name(field);
                let param_type = self.get_field_param_type(field);
                code.push_str(&format!(", {}: {}", param_name, param_type));
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
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue; // Skip virtual fields
            }

            let field_name = self.get_field_param_name(field);
            code.push_str(&format!("            {},\n", field_name));
        }
        code.push_str("        };\n\n");

        // Add to indexes
        code.push_str("        let row_index = self.records.len();\n");
        for field in &model.fields {
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue;
            }

            let field_name = self.get_field_param_name(field);
            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique {
                // Unique index: map value to single row index
                code.push_str(&format!("        self.{}_index.insert(record.{}.clone(), row_index);\n",
                    field_name, field_name));
            } else if field.indexed || is_fk {
                // Non-unique index: append row index to list of indices
                code.push_str(&format!("        self.{}_index.entry(record.{}.clone()).or_insert_with(Vec::new).push(row_index);\n",
                    field_name, field_name));
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
        // Generate find_by_X for indexed or unique fields, and FK fields
        for field in &model.fields {
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue;
            }

            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.indexed || field.unique || is_fk {
                let param_name = self.get_field_param_name(field);
                let param_type = self.get_field_param_type(field);
                let method_name = format!("find_by_{}", param_name);

                code.push_str(&format!("    pub fn {}(&self, {}: {}) -> Vec<{}> {{\n",
                    method_name, param_name, param_type, model.name));

                if field.unique {
                    // Unique index: O(1) lookup, returns 0 or 1 results
                    code.push_str(&format!("        if let Some(&idx) = self.{}_index.get(&{}) {{\n", param_name, param_name));
                    code.push_str("            if !self.tombstones[idx] {\n");
                    code.push_str("                return vec![self.records[idx].clone()];\n");
                    code.push_str("            }\n");
                    code.push_str("        }\n");
                    code.push_str("        Vec::new()\n");
                } else {
                    // Non-unique index: O(1) lookup, may return multiple results
                    code.push_str(&format!("        if let Some(indices) = self.{}_index.get(&{}) {{\n", param_name, param_name));
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

            // Parameters: all non-auto-generated, non-OneToMany fields
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                    continue;
                }

                if !field.auto_generate {
                    let param_name = self.get_field_param_name(field);
                    let param_type = self.get_field_param_type(field);
                    code.push_str(&format!(", {}: {}", param_name, param_type));
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
                if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                    continue;
                }

                let param_name = self.get_field_param_name(field);
                let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!("        self.{}_index.remove(&self.records[idx].{});\n", param_name, param_name));
                } else if field.indexed || is_fk {
                    code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                    code.push_str("            indices.retain(|&i| i != idx);\n");
                    code.push_str("        }\n");
                }
            }

            // Check unique constraints for new values
            for field in &model.fields {
                if field.unique && !field.auto_generate {
                    let param_name = self.get_field_param_name(field);
                    code.push_str(&format!("        if self.{}_index.contains_key(&{}) && self.records[idx].{} != {} {{\n",
                        param_name, param_name, param_name, param_name));
                    code.push_str(&format!("            return Err(\"Unique constraint violation: {} already exists\".to_string());\n", param_name));
                    code.push_str("        }\n");
                }
            }

            // Update record
            code.push_str(&format!("        self.records[idx] = {} {{\n", model.name));
            code.push_str(&format!("            {}: self.records[idx].{}.clone(),\n", id_field.name, id_field.name));
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                    continue;
                }

                if !field.auto_generate {
                    let param_name = self.get_field_param_name(field);
                    code.push_str(&format!("            {},\n", param_name));
                }
            }
            code.push_str("        };\n\n");

            // Add new values to indexes
            for field in &model.fields {
                if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                    continue;
                }

                let param_name = self.get_field_param_name(field);
                let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!("        self.{}_index.insert(self.records[idx].{}.clone(), idx);\n", param_name, param_name));
                } else if field.indexed || is_fk {
                    code.push_str(&format!("        self.{}_index.entry(self.records[idx].{}.clone()).or_insert_with(Vec::new).push(idx);\n",
                        param_name, param_name));
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
                if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                    continue;
                }

                let param_name = self.get_field_param_name(field);
                let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!("        self.{}_index.remove(&self.records[idx].{});\n", param_name, param_name));
                } else if field.indexed || is_fk {
                    code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                    code.push_str("            indices.retain(|&i| i != idx);\n");
                    code.push_str("        }\n");
                }
            }

            code.push_str("        Ok(())\n");
            code.push_str("    }\n\n");
        }
    }

    fn generate_database_struct(&self, schema: &Schema) -> String {
        let mut code = String::new();

        code.push_str("pub struct Database {\n");
        for model in &schema.models {
            code.push_str(&format!("    pub {}: {}Storage,\n",
                model.name.to_lowercase(), model.name));
        }
        code.push_str("}\n\n");

        code.push_str("impl Database {\n");
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str("        Database {\n");
        for model in &schema.models {
            code.push_str(&format!("            {}: {}Storage::new(),\n",
                model.name.to_lowercase(), model.name));
        }
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // Generate FK validation insert methods
        for model in &schema.models {
            self.generate_db_insert_with_fk_validation(&mut code, model, schema);
        }

        // Generate relation traversal methods
        let relations = schema.detect_relations();
        for relation in &relations {
            self.generate_relation_traversal_method(&mut code, relation, schema);
            self.generate_reverse_lookup_method(&mut code, relation, schema);
        }

        code.push_str("}\n\n");

        code
    }

    fn generate_db_insert_with_fk_validation(&self, code: &mut String, model: &Model, schema: &Schema) {
        // Find FK fields in this model
        let fk_fields: Vec<&Field> = model.fields.iter()
            .filter(|f| matches!(&f.field_type, FieldType::Relation(rel) if rel.is_reference()))
            .collect();

        if fk_fields.is_empty() {
            return; // No FKs to validate
        }

        let method_name = format!("insert_{}", model.name.to_lowercase());
        code.push_str(&format!("    pub fn {}(&mut self", method_name));

        // Parameters: all fields except auto-generated and OneToMany
        for field in &model.fields {
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue;
            }

            if !field.auto_generate {
                let param_name = self.get_field_param_name(field);
                let param_type = self.get_field_param_type(field);
                code.push_str(&format!(", {}: {}", param_name, param_type));
            }
        }

        code.push_str(&format!(") -> Result<{}, String> {{\n", model.name));

        // Validate each FK
        for field in &fk_fields {
            if let FieldType::Relation(rel) = &field.field_type {
                let target_model = rel.target_model();
                let fk_param = format!("{}_id", field.name);
                let storage_name = target_model.to_lowercase();

                match rel {
                    RelationType::RequiredReference(_) => {
                        code.push_str(&format!("        if self.{}.get({}).is_none() {{\n", storage_name, fk_param));
                        code.push_str(&format!("            return Err(\"Foreign key validation failed: {} does not exist\".to_string());\n", target_model));
                        code.push_str("        }\n");
                    }
                    RelationType::OptionalReference(_) => {
                        code.push_str(&format!("        if let Some(fk) = {} {{\n", fk_param));
                        code.push_str(&format!("            if self.{}.get(fk).is_none() {{\n", storage_name));
                        code.push_str(&format!("                return Err(\"Foreign key validation failed: {} does not exist\".to_string());\n", target_model));
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                    }
                    _ => {}
                }
            }
        }

        // Call the underlying storage insert
        code.push_str(&format!("        self.{}.insert(", model.name.to_lowercase()));
        let mut first = true;
        for field in &model.fields {
            if let FieldType::Relation(RelationType::OneToMany(_)) = &field.field_type {
                continue;
            }
            if !field.auto_generate {
                if !first {
                    code.push_str(", ");
                }
                code.push_str(&self.get_field_param_name(field));
                first = false;
            }
        }
        code.push_str(")\n");
        code.push_str("    }\n\n");
    }

    fn generate_relation_traversal_method(&self, code: &mut String, relation: &crate::ast::RelationPair, schema: &Schema) {
        // Generate parent.children() method
        // e.g., user.posts() -> Vec<Post>
        let parent_storage = relation.parent_model.to_lowercase();
        let child_storage = relation.child_model.to_lowercase();
        let method_name = format!("{}_{}",
            parent_storage,
            relation.parent_field); // e.g., user_posts

        let parent_model = schema.find_model(&relation.parent_model).unwrap();
        let id_field = parent_model.fields.iter().find(|f| f.auto_generate).unwrap();

        code.push_str(&format!("    pub fn {}(&self, {}_id: {}) -> Vec<{}> {{\n",
            method_name,
            parent_storage,
            id_field.field_type.to_rust_type(),
            relation.child_model));

        code.push_str(&format!("        self.{}.find_by_{}_id({}_id)\n",
            child_storage,
            parent_storage,
            parent_storage));

        code.push_str("    }\n\n");
    }

    fn generate_reverse_lookup_method(&self, code: &mut String, relation: &crate::ast::RelationPair, schema: &Schema) {
        // Generate child.parent() method
        // e.g., post.author() -> Option<User>
        let parent_storage = relation.parent_model.to_lowercase();
        let child_storage = relation.child_model.to_lowercase();
        let method_name = format!("{}_{}",
            child_storage,
            relation.child_field); // e.g., post_author

        let child_model = schema.find_model(&relation.child_model).unwrap();
        let id_field = child_model.fields.iter().find(|f| f.auto_generate).unwrap();

        let return_type = if relation.is_required {
            format!("Option<{}>", relation.parent_model)
        } else {
            format!("Option<{}>", relation.parent_model)
        };

        code.push_str(&format!("    pub fn {}(&self, {}_id: {}) -> {} {{\n",
            method_name,
            child_storage,
            id_field.field_type.to_rust_type(),
            return_type));

        code.push_str(&format!("        if let Some(child) = self.{}.get({}_id) {{\n",
            child_storage,
            child_storage));

        code.push_str(&format!("            return self.{}.get(child.{}_id);\n",
            parent_storage,
            relation.child_field));

        code.push_str("        }\n");
        code.push_str("        None\n");
        code.push_str("    }\n\n");
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

        // Generate Database struct that holds all storages
        code.push_str(&self.generate_database_struct(schema));

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

    // Sprint 4: Relation tests
    #[test]
    fn test_generate_relation_one_to_many() {
        let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // User struct should NOT have posts field (virtual)
        assert!(code.contains("pub struct User"));
        assert!(!code.contains("pub posts:"));
        assert!(code.contains("pub id: uuid::Uuid"));
    }

    #[test]
    fn test_generate_relation_required_reference() {
        let input = r#"
User {
  id: +uuid
}

Post {
  id: +uuid
  author: *User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Post struct should have author_id field (FK)
        assert!(code.contains("pub struct Post"));
        assert!(code.contains("pub author_id: uuid::Uuid"));

        // PostStorage should have author_id_index
        assert!(code.contains("pub struct PostStorage"));
        assert!(code.contains("author_id_index: std::collections::HashMap<uuid::Uuid, Vec<usize>>"));

        // Insert should take author_id parameter
        assert!(code.contains("pub fn insert(&mut self, author_id: uuid::Uuid)"));

        // Should generate find_by_author_id method
        assert!(code.contains("pub fn find_by_author_id(&self, author_id: uuid::Uuid)"));

        // author_id should be indexed
        assert!(code.contains("self.author_id_index.entry(record.author_id.clone()).or_insert_with(Vec::new).push(row_index)"));
    }

    #[test]
    fn test_generate_relation_optional_reference() {
        let input = r#"
User {
  id: +uuid
}

Post {
  id: +uuid
  reviewer: ?User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Post struct should have optional reviewer_id field
        assert!(code.contains("pub struct Post"));
        assert!(code.contains("pub reviewer_id: Option<uuid::Uuid>"));

        // Should generate find_by_reviewer_id method
        assert!(code.contains("pub fn find_by_reviewer_id(&self, reviewer_id: Option<uuid::Uuid>)"));
    }

    #[test]
    fn test_generate_full_relation_schema() {
        let input = r#"
User {
  id: +uuid
  email: ^&string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // User struct
        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub id: uuid::Uuid"));
        assert!(code.contains("pub email: String"));
        assert!(!code.contains("pub posts:"));

        // Post struct with FK
        assert!(code.contains("pub struct Post"));
        assert!(code.contains("pub title: String"));
        assert!(code.contains("pub author_id: uuid::Uuid"));

        // Indexes
        assert!(code.contains("email_index: std::collections::HashMap<String, usize>"));
        assert!(code.contains("author_id_index: std::collections::HashMap<uuid::Uuid, Vec<usize>>"));

        // Methods
        assert!(code.contains("pub fn find_by_email(&self, email: String)"));
        assert!(code.contains("pub fn find_by_author_id(&self, author_id: uuid::Uuid)"));
    }

    #[test]
    fn test_detect_relations() {
        let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
  author: *User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let relations = schema.detect_relations();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].parent_model, "User");
        assert_eq!(relations[0].parent_field, "posts");
        assert_eq!(relations[0].child_model, "Post");
        assert_eq!(relations[0].child_field, "author");
        assert!(relations[0].is_required);
    }

    // Sprint 4.1: Database struct tests
    #[test]
    fn test_generate_database_struct() {
        let input = r#"
User {
  id: +uuid
}

Post {
  id: +uuid
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check Database struct
        assert!(code.contains("pub struct Database"));
        assert!(code.contains("pub user: UserStorage"));
        assert!(code.contains("pub post: PostStorage"));

        // Check Database::new()
        assert!(code.contains("pub fn new() -> Self"));
        assert!(code.contains("user: UserStorage::new()"));
        assert!(code.contains("post: PostStorage::new()"));
    }

    #[test]
    fn test_generate_fk_validation() {
        let input = r#"
User {
  id: +uuid
}

Post {
  id: +uuid
  author: *User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check FK validation method
        assert!(code.contains("pub fn insert_post("));
        assert!(code.contains("if self.user.get(author_id).is_none()"));
        assert!(code.contains("Foreign key validation failed: User does not exist"));
        assert!(code.contains("self.post.insert("));
    }

    #[test]
    fn test_generate_relation_traversal() {
        let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
  author: *User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check relation traversal method
        assert!(code.contains("pub fn user_posts(&self, user_id: uuid::Uuid) -> Vec<Post>"));
        assert!(code.contains("self.post.find_by_user_id(user_id)"));
    }

    #[test]
    fn test_generate_reverse_lookup() {
        let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
  author: *User
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check reverse lookup method
        assert!(code.contains("pub fn post_author(&self, post_id: uuid::Uuid) -> Option<User>"));
        assert!(code.contains("if let Some(child) = self.post.get(post_id)"));
        assert!(code.contains("self.user.get(child.author_id)"));
    }
}
