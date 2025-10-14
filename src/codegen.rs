use crate::ast::{
    ConstraintParam, Field, FieldType, IndexType, ManyToManyRelation, Model, RelationType, Schema,
    Struct,
};

pub struct CodeGenerator;

/// Represents a generated file
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
}

impl CodeGenerator {
    pub fn new() -> Self {
        CodeGenerator
    }

    /// Check if a field is virtual (OneToMany or ManyToMany) and doesn't need storage
    fn is_virtual_field(field: &Field) -> bool {
        matches!(
            &field.field_type,
            FieldType::Relation(RelationType::OneToMany(_))
                | FieldType::Relation(RelationType::ManyToMany(_))
        )
    }

    /// Generate struct definition (Sprint 8)
    fn generate_struct_definition(&self, struct_def: &Struct) -> String {
        let mut code = String::new();

        code.push_str(&format!("#[derive(Debug, Clone, Copy, PartialEq)]\n"));
        code.push_str("#[repr(C)]\n"); // Ensure C layout for predictable memory layout
        code.push_str(&format!("pub struct {} {{\n", struct_def.name));

        for field in &struct_def.fields {
            let rust_type = field.field_type.to_rust_type();
            code.push_str(&format!("    pub {}: {},\n", field.name, rust_type));
        }

        code.push_str("}\n\n");

        // Generate helper methods for struct
        code.push_str(&format!("impl {} {{\n", struct_def.name));

        // Generate constructor
        code.push_str("    pub fn new(");
        for (i, field) in struct_def.fields.iter().enumerate() {
            if i > 0 {
                code.push_str(", ");
            }
            code.push_str(&format!(
                "{}: {}",
                field.name,
                field.field_type.to_rust_type()
            ));
        }
        code.push_str(") -> Self {\n");
        code.push_str(&format!("        {} {{\n", struct_def.name));
        for field in &struct_def.fields {
            code.push_str(&format!("            {},\n", field.name));
        }
        code.push_str("        }\n");
        code.push_str("    }\n");
        code.push_str("}\n\n");

        code
    }

    fn generate_field_declaration(&self, field: &Field) -> String {
        // Skip virtual fields - they don't store data
        if Self::is_virtual_field(field) {
            return String::new();
        }

        // For reference fields, generate the FK field name
        let (field_name, rust_type) = match &field.field_type {
            FieldType::Relation(RelationType::RequiredReference(_)) => {
                (format!("{}_id", field.name), "uuid::Uuid".to_string())
            }
            FieldType::Relation(RelationType::OptionalReference(_)) => (
                format!("{}_id", field.name),
                "Option<uuid::Uuid>".to_string(),
            ),
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

    /// Generate computed field trait for a model (Sprint 12)
    fn generate_computed_trait(&self, model: &Model) -> String {
        let computed_fields: Vec<&Field> = model.fields.iter().filter(|f| f.is_computed).collect();

        if computed_fields.is_empty() {
            return String::new();
        }

        let mut code = String::new();

        code.push_str(&format!("/// Computed fields trait for {}\n", model.name));
        code.push_str(&format!("pub trait {}Computed {{\n", model.name));

        for field in &computed_fields {
            // Determine parameters based on field dependencies
            // For now, we pass a reference to the entire model instance
            code.push_str(&format!("    /// Compute the value of '{}'\n", field.name));
            code.push_str(&format!(
                "    fn {}(instance: &{}) -> {};\n",
                field.name,
                model.name,
                field.field_type.to_rust_type()
            ));
        }

        code.push_str("}\n\n");

        // Generate a default stub implementation
        code.push_str(&format!(
            "/// Default stub implementation for {}Computed\n",
            model.name
        ));
        code.push_str(&format!("pub struct Default{}Computed;\n\n", model.name));
        code.push_str(&format!(
            "impl {}Computed for Default{}Computed {{\n",
            model.name, model.name
        ));

        for field in &computed_fields {
            code.push_str(&format!(
                "    fn {}(instance: &{}) -> {} {{\n",
                field.name,
                model.name,
                field.field_type.to_rust_type()
            ));
            code.push_str("        // TODO: Implement computation logic\n");

            // Generate a placeholder return value based on type
            let default_value = match &field.field_type {
                FieldType::String => "String::new()".to_string(),
                FieldType::U32 => "0u32".to_string(),
                FieldType::U64 => "0u64".to_string(),
                FieldType::I32 => "0i32".to_string(),
                FieldType::I64 => "0i64".to_string(),
                FieldType::F64 => "0.0f64".to_string(),
                FieldType::Bool => "false".to_string(),
                FieldType::Uuid => "Uuid::nil()".to_string(),
                _ => "unimplemented!()".to_string(),
            };
            code.push_str(&format!("        {}\n", default_value));
            code.push_str("    }\n");
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
            // Skip virtual fields
            if Self::is_virtual_field(field) {
                continue;
            }

            // Get the actual field name and type for storage
            let (field_name, field_type) = match &field.field_type {
                FieldType::Relation(RelationType::RequiredReference(_)) => {
                    (format!("{}_id", field.name), "uuid::Uuid".to_string())
                }
                FieldType::Relation(RelationType::OptionalReference(_)) => (
                    format!("{}_id", field.name),
                    "Option<uuid::Uuid>".to_string(),
                ),
                _ => (field.name.clone(), field.field_type.to_rust_type()),
            };

            // FK fields are automatically indexed (non-unique)
            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique {
                // Unique index (& symbol) - maps value to single row index
                code.push_str(&format!(
                    "    {}_index: std::collections::HashMap<{}, usize>,\n",
                    field_name, field_type
                ));
            } else if field.indexed || is_fk {
                // Indexed fields use either Hash or BTree based on field type
                match field.index_type {
                    IndexType::Hash => {
                        // Hash index for unordered types
                        code.push_str(&format!(
                            "    {}_index: std::collections::HashMap<{}, Vec<usize>>,\n",
                            field_name, field_type
                        ));
                    }
                    IndexType::BTree => {
                        // B-tree index for ordered types (supports range queries)
                        let btree_key_type = if matches!(field.field_type, FieldType::F64) {
                            "ordered_float::OrderedFloat<f64>".to_string()
                        } else {
                            field_type.clone()
                        };
                        code.push_str(&format!(
                            "    {}_btree: std::collections::BTreeMap<{}, Vec<usize>>,\n",
                            field_name, btree_key_type
                        ));
                    }
                }
            }
        }

        // Add composite indexes
        for comp_idx in &model.composite_indexes {
            let index_name = comp_idx.fields.join("_");
            let field_types: Vec<String> =
                comp_idx
                    .fields
                    .iter()
                    .filter_map(|fname| {
                        model.fields.iter().find(|f| &f.name == fname).map(|f| {
                            match &f.field_type {
                                FieldType::Relation(RelationType::RequiredReference(_)) => {
                                    "uuid::Uuid".to_string()
                                }
                                FieldType::Relation(RelationType::OptionalReference(_)) => {
                                    "Option<uuid::Uuid>".to_string()
                                }
                                _ => f.field_type.to_rust_type(),
                            }
                        })
                    })
                    .collect();
            let tuple_type = format!("({})", field_types.join(", "));
            code.push_str(&format!(
                "    {}_index: std::collections::HashMap<{}, Vec<usize>>,\n",
                index_name, tuple_type
            ));
        }

        // Add full-text search indexes (Sprint 18)
        let has_fulltext = model.fields.iter().any(|f| f.fulltext_indexed);
        if has_fulltext {
            code.push_str("    // Full-text search indexes\n");
            for field in &model.fields {
                if field.fulltext_indexed {
                    code.push_str(&format!("    {}_fulltext: std::sync::Arc<std::sync::RwLock<sinkdb_fulltext::FullTextIndex>>,\n",
                        field.name));
                }
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
            // Skip virtual fields
            if Self::is_virtual_field(field) {
                continue;
            }

            let field_name = match &field.field_type {
                FieldType::Relation(rel) if rel.is_reference() => format!("{}_id", field.name),
                _ => field.name.clone(),
            };

            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique {
                code.push_str(&format!(
                    "            {}_index: std::collections::HashMap::new(),\n",
                    field_name
                ));
            } else if field.indexed || is_fk {
                match field.index_type {
                    IndexType::Hash => {
                        code.push_str(&format!(
                            "            {}_index: std::collections::HashMap::new(),\n",
                            field_name
                        ));
                    }
                    IndexType::BTree => {
                        code.push_str(&format!(
                            "            {}_btree: std::collections::BTreeMap::new(),\n",
                            field_name
                        ));
                    }
                }
            }
        }

        // Initialize composite indexes
        for comp_idx in &model.composite_indexes {
            let index_name = comp_idx.fields.join("_");
            code.push_str(&format!(
                "            {}_index: std::collections::HashMap::new(),\n",
                index_name
            ));
        }

        // Initialize full-text indexes (Sprint 18)
        for field in &model.fields {
            if field.fulltext_indexed {
                code.push_str(&format!("            {}_fulltext: std::sync::Arc::new(std::sync::RwLock::new(sinkdb_fulltext::FullTextIndex::new())),\n",
                    field.name));
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

        // Generate full-text search methods (Sprint 18)
        self.generate_search_methods(&mut code, model);

        // Generate update() method
        self.generate_update_method(&mut code, model);

        // Generate delete() method
        self.generate_delete_method(&mut code, model);

        // Generate computed field accessors (Sprint 12)
        self.generate_computed_accessors(&mut code, model);

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
            FieldType::Relation(RelationType::OptionalReference(_)) => {
                "Option<uuid::Uuid>".to_string()
            }
            _ => field.field_type.to_rust_type(),
        }
    }

    fn generate_validation_functions(&self) -> String {
        let mut code = String::new();

        // Email validation function
        code.push_str(
            r#"fn validate_email(value: &str) -> Result<(), String> {
    let email_regex = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
    if !regex::Regex::new(email_regex).unwrap().is_match(value) {
        return Err(format!("'{}' is not a valid email address", value));
    }
    Ok(())
}

"#,
        );

        // URL validation function
        code.push_str(
            r#"fn validate_url(value: &str) -> Result<(), String> {
    let url_regex = r"^https?://[^\s/$.?#].[^\s]*$";
    if !regex::Regex::new(url_regex).unwrap().is_match(value) {
        return Err(format!("'{}' is not a valid URL", value));
    }
    Ok(())
}

"#,
        );

        // Pattern validation function (generic)
        code.push_str(
            r#"fn validate_pattern(value: &str, pattern: &str) -> Result<(), String> {
    if !regex::Regex::new(pattern).unwrap().is_match(value) {
        return Err(format!("'{}' does not match required pattern", value));
    }
    Ok(())
}

"#,
        );

        code
    }

    fn generate_field_validation(&self, field: &Field) -> String {
        let mut code = String::new();

        // Skip validation for relations
        if matches!(&field.field_type, FieldType::Relation(_)) {
            return code;
        }

        for constraint in &field.constraints {
            match constraint.name.as_str() {
                "email" => {
                    if matches!(field.field_type, FieldType::String) {
                        code.push_str(&format!("        validate_email(&{})?;\n", field.name));
                    }
                }
                "url" => {
                    if matches!(field.field_type, FieldType::String) {
                        code.push_str(&format!("        validate_url(&{})?;\n", field.name));
                    }
                }
                "min" => {
                    if let Some(ConstraintParam::Number(min_val)) = constraint.params.first() {
                        match field.field_type {
                            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => {
                                code.push_str(&format!(
                                    "        if {} < {} {{\n",
                                    field.name, min_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at least {}\".to_string());\n", field.name, min_val));
                                code.push_str("        }\n");
                            }
                            FieldType::String => {
                                // For strings, min means minimum length
                                code.push_str(&format!(
                                    "        if {}.len() < {} {{\n",
                                    field.name, min_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at least {} characters\".to_string());\n", field.name, min_val));
                                code.push_str("        }\n");
                            }
                            _ => {}
                        }
                    }
                }
                "max" => {
                    if let Some(ConstraintParam::Number(max_val)) = constraint.params.first() {
                        match field.field_type {
                            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => {
                                code.push_str(&format!(
                                    "        if {} > {} {{\n",
                                    field.name, max_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at most {}\".to_string());\n", field.name, max_val));
                                code.push_str("        }\n");
                            }
                            FieldType::String => {
                                // For strings, max means maximum length
                                code.push_str(&format!(
                                    "        if {}.len() > {} {{\n",
                                    field.name, max_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at most {} characters\".to_string());\n", field.name, max_val));
                                code.push_str("        }\n");
                            }
                            _ => {}
                        }
                    }
                }
                "pattern" => {
                    if let Some(ConstraintParam::String(pattern)) = constraint.params.first() {
                        if matches!(field.field_type, FieldType::String) {
                            code.push_str(&format!(
                                "        validate_pattern(&{}, \"{}\")?;\n",
                                field.name, pattern
                            ));
                        }
                    }
                }
                _ => {
                    // Unknown constraints are ignored for now
                }
            }
        }

        code
    }

    fn generate_insert_method(&self, code: &mut String, model: &Model) {
        // Find unique fields
        let unique_fields: Vec<&Field> = model.fields.iter().filter(|f| f.unique).collect();

        code.push_str("    pub fn insert(&mut self");

        // Parameters: all fields except auto-generated and virtual ones
        for field in &model.fields {
            if Self::is_virtual_field(field) {
                continue; // Skip virtual fields
            }

            if !field.auto_generate {
                let param_name = self.get_field_param_name(field);
                let param_type = self.get_field_param_type(field);
                code.push_str(&format!(", {}: {}", param_name, param_type));
            }
        }

        code.push_str(&format!(") -> Result<{}, String> {{\n", model.name));

        // Validate field constraints
        for field in &model.fields {
            if !field.auto_generate && !field.constraints.is_empty() {
                let validation = self.generate_field_validation(field);
                if !validation.is_empty() {
                    code.push_str(&validation);
                }
            }
        }

        // Check unique constraints
        for field in &unique_fields {
            code.push_str(&format!(
                "        if self.{}_index.contains_key(&{}) {{\n",
                field.name, field.name
            ));
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
            if Self::is_virtual_field(field) {
                continue; // Skip virtual fields
            }

            let field_name = self.get_field_param_name(field);
            code.push_str(&format!("            {},\n", field_name));
        }
        code.push_str("        };\n\n");

        // Add to indexes
        code.push_str("        let row_index = self.records.len();\n");
        for field in &model.fields {
            if Self::is_virtual_field(field) {
                continue;
            }

            let field_name = self.get_field_param_name(field);
            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.unique {
                // Unique index: map value to single row index
                code.push_str(&format!(
                    "        self.{}_index.insert(record.{}.clone(), row_index);\n",
                    field_name, field_name
                ));
            } else if field.indexed || is_fk {
                match field.index_type {
                    IndexType::Hash => {
                        // Hash index: append row index to list of indices
                        code.push_str(&format!("        self.{}_index.entry(record.{}.clone()).or_insert_with(Vec::new).push(row_index);\n",
                            field_name, field_name));
                    }
                    IndexType::BTree => {
                        // B-tree index: append row index to list of indices
                        if matches!(field.field_type, FieldType::F64) {
                            code.push_str(&format!("        self.{}_btree.entry(ordered_float::OrderedFloat(record.{})).or_insert_with(Vec::new).push(row_index);\n",
                                field_name, field_name));
                        } else {
                            code.push_str(&format!("        self.{}_btree.entry(record.{}.clone()).or_insert_with(Vec::new).push(row_index);\n",
                                field_name, field_name));
                        }
                    }
                }
            }
        }

        // Add to composite indexes
        for comp_idx in &model.composite_indexes {
            let index_name = comp_idx.fields.join("_");
            let field_values: Vec<String> = comp_idx
                .fields
                .iter()
                .map(|fname| {
                    let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                    let field_name = self.get_field_param_name(field);
                    format!("record.{}.clone()", field_name)
                })
                .collect();
            let tuple_value = format!("({})", field_values.join(", "));
            code.push_str(&format!(
                "        self.{}_index.entry({}).or_insert_with(Vec::new).push(row_index);\n",
                index_name, tuple_value
            ));
        }

        // Add to full-text indexes (Sprint 18)
        // Get the ID field to use as document ID
        let id_field = model.fields.iter().find(|f| f.auto_generate);
        for field in &model.fields {
            if field.fulltext_indexed {
                if let Some(id_field) = id_field {
                    if matches!(id_field.field_type, FieldType::Uuid) {
                        code.push_str(&format!("        self.{}_fulltext.write().unwrap().add_document(record.{}, &record.{});\n",
                            field.name, id_field.name, field.name));
                    }
                }
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
            code.push_str(&format!(
                "    pub fn get(&self, {}: {}) -> Option<{}> {{\n",
                id_field.name,
                id_field.field_type.to_rust_type(),
                model.name
            ));
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
            if Self::is_virtual_field(field) {
                continue;
            }

            let is_fk = matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

            if field.indexed || field.unique || is_fk {
                let param_name = self.get_field_param_name(field);
                let param_type = self.get_field_param_type(field);
                let method_name = format!("find_by_{}", param_name);

                code.push_str(&format!(
                    "    pub fn {}(&self, {}: {}) -> Vec<{}> {{\n",
                    method_name, param_name, param_type, model.name
                ));

                if field.unique {
                    // Unique index: O(1) lookup, returns 0 or 1 results
                    code.push_str(&format!(
                        "        if let Some(&idx) = self.{}_index.get(&{}) {{\n",
                        param_name, param_name
                    ));
                    code.push_str("            if !self.tombstones[idx] {\n");
                    code.push_str("                return vec![self.records[idx].clone()];\n");
                    code.push_str("            }\n");
                    code.push_str("        }\n");
                    code.push_str("        Vec::new()\n");
                } else {
                    match field.index_type {
                        IndexType::Hash => {
                            // Hash index: O(1) lookup, may return multiple results
                            code.push_str(&format!(
                                "        if let Some(indices) = self.{}_index.get(&{}) {{\n",
                                param_name, param_name
                            ));
                            code.push_str("            return indices.iter()\n");
                            code.push_str(
                                "                .filter(|&&idx| !self.tombstones[idx])\n",
                            );
                            code.push_str(
                                "                .map(|&idx| self.records[idx].clone())\n",
                            );
                            code.push_str("                .collect();\n");
                            code.push_str("        }\n");
                            code.push_str("        Vec::new()\n");
                        }
                        IndexType::BTree => {
                            // B-tree index: O(log n) lookup
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get(&ordered_float::OrderedFloat({})) {{\n", param_name, param_name));
                            } else {
                                code.push_str(&format!(
                                    "        if let Some(indices) = self.{}_btree.get(&{}) {{\n",
                                    param_name, param_name
                                ));
                            }
                            code.push_str("            return indices.iter()\n");
                            code.push_str(
                                "                .filter(|&&idx| !self.tombstones[idx])\n",
                            );
                            code.push_str(
                                "                .map(|&idx| self.records[idx].clone())\n",
                            );
                            code.push_str("                .collect();\n");
                            code.push_str("        }\n");
                            code.push_str("        Vec::new()\n");
                        }
                    }
                }

                code.push_str("    }\n\n");

                // Generate range query methods for B-tree indexed fields
                if !field.unique && matches!(field.index_type, IndexType::BTree) {
                    self.generate_range_query_methods(code, field, model);
                }
            }
        }

        // Generate find_by methods for composite indexes
        for comp_idx in &model.composite_indexes {
            self.generate_composite_find_by_method(code, comp_idx, model);
        }
    }

    fn generate_range_query_methods(&self, code: &mut String, field: &Field, model: &Model) {
        let param_name = self.get_field_param_name(field);
        let param_type = self.get_field_param_type(field);

        // find_by_X_range(min, max)
        code.push_str(&format!(
            "    pub fn find_by_{}_range(&self, min: {}, max: {}) -> Vec<{}> {{\n",
            param_name, param_type, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(ordered_float::OrderedFloat(min)..=ordered_float::OrderedFloat(max)) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(min..=max) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_gt(min) - greater than
        code.push_str(&format!(
            "    pub fn find_by_{}_gt(&self, min: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range((std::ops::Bound::Excluded(ordered_float::OrderedFloat(min)), std::ops::Bound::Unbounded)) {{\n", param_name));
        } else {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range((std::ops::Bound::Excluded(min), std::ops::Bound::Unbounded)) {{\n", param_name));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_gte(min) - greater than or equal
        code.push_str(&format!(
            "    pub fn find_by_{}_gte(&self, min: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(ordered_float::OrderedFloat(min)..) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(min..) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_lt(max) - less than
        code.push_str(&format!(
            "    pub fn find_by_{}_lt(&self, max: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(..ordered_float::OrderedFloat(max)) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(..max) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");

        // find_by_X_lte(max) - less than or equal
        code.push_str(&format!(
            "    pub fn find_by_{}_lte(&self, max: {}) -> Vec<{}> {{\n",
            param_name, param_type, model.name
        ));
        code.push_str("        let mut results = Vec::new();\n");
        if matches!(field.field_type, FieldType::F64) {
            code.push_str(&format!("        for (_key, indices) in self.{}_btree.range(..=ordered_float::OrderedFloat(max)) {{\n", param_name));
        } else {
            code.push_str(&format!(
                "        for (_key, indices) in self.{}_btree.range(..=max) {{\n",
                param_name
            ));
        }
        code.push_str("            for &idx in indices {\n");
        code.push_str("                if !self.tombstones[idx] {\n");
        code.push_str("                    results.push(self.records[idx].clone());\n");
        code.push_str("                }\n");
        code.push_str("            }\n");
        code.push_str("        }\n");
        code.push_str("        results\n");
        code.push_str("    }\n\n");
    }

    fn generate_composite_find_by_method(
        &self,
        code: &mut String,
        comp_idx: &crate::ast::CompositeIndex,
        model: &Model,
    ) {
        let method_name = format!(
            "find_by_{}",
            comp_idx
                .fields
                .iter()
                .map(|f| format!("{}", f))
                .collect::<Vec<_>>()
                .join("_and_")
        );
        let index_name = comp_idx.fields.join("_");

        // Generate parameters
        let params: Vec<String> = comp_idx
            .fields
            .iter()
            .map(|fname| {
                let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                let param_name = self.get_field_param_name(field);
                let param_type = self.get_field_param_type(field);
                format!("{}: {}", param_name, param_type)
            })
            .collect();

        code.push_str(&format!(
            "    pub fn {}(&self, {}) -> Vec<{}> {{\n",
            method_name,
            params.join(", "),
            model.name
        ));

        // Generate tuple key
        let tuple_values: Vec<String> = comp_idx
            .fields
            .iter()
            .map(|fname| {
                let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                self.get_field_param_name(field)
            })
            .collect();
        let tuple_key = format!("({})", tuple_values.join(", "));

        code.push_str(&format!(
            "        if let Some(indices) = self.{}_index.get(&{}) {{\n",
            index_name, tuple_key
        ));
        code.push_str("            return indices.iter()\n");
        code.push_str("                .filter(|&&idx| !self.tombstones[idx])\n");
        code.push_str("                .map(|&idx| self.records[idx].clone())\n");
        code.push_str("                .collect();\n");
        code.push_str("        }\n");
        code.push_str("        Vec::new()\n");
        code.push_str("    }\n\n");
    }

    fn generate_list_method(&self, code: &mut String, model: &Model) {
        code.push_str(&format!(
            "    pub fn list(&self) -> Vec<{}> {{\n",
            model.name
        ));
        code.push_str("        self.records.iter().enumerate()\n");
        code.push_str("            .filter(|(i, _)| !self.tombstones[*i])\n");
        code.push_str("            .map(|(_, r)| r.clone())\n");
        code.push_str("            .collect()\n");
        code.push_str("    }\n\n");
    }

    fn generate_search_methods(&self, code: &mut String, model: &Model) {
        // Generate search methods for full-text indexed fields (Sprint 18)
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        for field in &model.fields {
            if field.fulltext_indexed {
                // Only generate if we have a UUID ID field
                if let Some(id_field) = id_field {
                    if matches!(id_field.field_type, FieldType::Uuid) {
                        // Generate search method
                        code.push_str(&format!(
                            "    /// Full-text search on the '{}' field\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "    pub fn search_{}(&self, query: &str) -> Vec<{}> {{\n",
                            field.name, model.name
                        ));
                        code.push_str(&format!("        let matches = self.{}_fulltext.read().unwrap().search(query);\n", field.name));
                        code.push_str("        let mut results = Vec::new();\n");
                        code.push_str("        for doc_match in matches {\n");
                        code.push_str(
                            "            if let Some(record) = self.records.iter().enumerate()\n",
                        );
                        code.push_str(&format!("                .find(|(i, r)| !self.tombstones[*i] && r.{} == doc_match.doc_id)\n", id_field.name));
                        code.push_str("                .map(|(_, r)| r.clone()) {\n");
                        code.push_str("                results.push(record);\n");
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                        code.push_str("        results\n");
                        code.push_str("    }\n\n");

                        // Generate phrase search method
                        code.push_str(&format!(
                            "    /// Phrase search on the '{}' field\n",
                            field.name
                        ));
                        code.push_str(&format!(
                            "    pub fn search_{}_phrase(&self, phrase: &str) -> Vec<{}> {{\n",
                            field.name, model.name
                        ));
                        code.push_str(&format!("        let matches = self.{}_fulltext.read().unwrap().search_phrase(phrase);\n", field.name));
                        code.push_str("        let mut results = Vec::new();\n");
                        code.push_str("        for doc_match in matches {\n");
                        code.push_str(
                            "            if let Some(record) = self.records.iter().enumerate()\n",
                        );
                        code.push_str(&format!("                .find(|(i, r)| !self.tombstones[*i] && r.{} == doc_match.doc_id)\n", id_field.name));
                        code.push_str("                .map(|(_, r)| r.clone()) {\n");
                        code.push_str("                results.push(record);\n");
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                        code.push_str("        results\n");
                        code.push_str("    }\n\n");
                    }
                }
            }
        }
    }

    fn generate_update_method(&self, code: &mut String, model: &Model) {
        // Find the ID field
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!(
                "    pub fn update(&mut self, {}: {}",
                id_field.name,
                id_field.field_type.to_rust_type()
            ));

            // Parameters: all non-auto-generated, non-virtual fields
            for field in &model.fields {
                if Self::is_virtual_field(field) {
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
            code.push_str(&format!(
                "            .find(|(i, r)| !self.tombstones[*i] && r.{} == {})\n",
                id_field.name, id_field.name
            ));
            code.push_str("            .map(|(i, _)| i)\n");
            code.push_str("            .ok_or_else(|| \"Record not found\".to_string())?;\n\n");

            // Remove old values from indexes
            for field in &model.fields {
                if Self::is_virtual_field(field) {
                    continue;
                }

                let param_name = self.get_field_param_name(field);
                let is_fk =
                    matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!(
                        "        self.{}_index.remove(&self.records[idx].{});\n",
                        param_name, param_name
                    ));
                } else if field.indexed || is_fk {
                    match field.index_type {
                        IndexType::Hash => {
                            code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                            code.push_str("            indices.retain(|&i| i != idx);\n");
                            code.push_str("        }\n");
                        }
                        IndexType::BTree => {
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get_mut(&ordered_float::OrderedFloat(self.records[idx].{})) {{\n", param_name, param_name));
                            } else {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                            }
                            code.push_str("            indices.retain(|&i| i != idx);\n");
                            code.push_str("        }\n");
                        }
                    }
                }
            }

            // Remove old values from composite indexes
            for comp_idx in &model.composite_indexes {
                let index_name = comp_idx.fields.join("_");
                let old_tuple_values: Vec<String> = comp_idx
                    .fields
                    .iter()
                    .map(|fname| {
                        let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                        let field_name = self.get_field_param_name(field);
                        format!("self.records[idx].{}.clone()", field_name)
                    })
                    .collect();
                let old_tuple = format!("({})", old_tuple_values.join(", "));
                code.push_str(&format!(
                    "        if let Some(indices) = self.{}_index.get_mut(&{}) {{\n",
                    index_name, old_tuple
                ));
                code.push_str("            indices.retain(|&i| i != idx);\n");
                code.push_str("        }\n");
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
            code.push_str(&format!(
                "            {}: self.records[idx].{}.clone(),\n",
                id_field.name, id_field.name
            ));
            for field in &model.fields {
                if Self::is_virtual_field(field) {
                    continue;
                }

                if field.auto_generate && field.name != id_field.name {
                    // Preserve auto-generated fields (except ID which is already handled)
                    code.push_str(&format!(
                        "            {}: self.records[idx].{}.clone(),\n",
                        field.name, field.name
                    ));
                } else if !field.auto_generate {
                    // Use parameter values for non-auto-generated fields
                    let param_name = self.get_field_param_name(field);
                    code.push_str(&format!("            {},\n", param_name));
                }
            }
            code.push_str("        };\n\n");

            // Add new values to indexes
            for field in &model.fields {
                if Self::is_virtual_field(field) {
                    continue;
                }

                let param_name = self.get_field_param_name(field);
                let is_fk =
                    matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!(
                        "        self.{}_index.insert(self.records[idx].{}.clone(), idx);\n",
                        param_name, param_name
                    ));
                } else if field.indexed || is_fk {
                    match field.index_type {
                        IndexType::Hash => {
                            code.push_str(&format!("        self.{}_index.entry(self.records[idx].{}.clone()).or_insert_with(Vec::new).push(idx);\n",
                                param_name, param_name));
                        }
                        IndexType::BTree => {
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        self.{}_btree.entry(ordered_float::OrderedFloat(self.records[idx].{})).or_insert_with(Vec::new).push(idx);\n",
                                    param_name, param_name));
                            } else {
                                code.push_str(&format!("        self.{}_btree.entry(self.records[idx].{}.clone()).or_insert_with(Vec::new).push(idx);\n",
                                    param_name, param_name));
                            }
                        }
                    }
                }
            }

            // Add new values to composite indexes
            for comp_idx in &model.composite_indexes {
                let index_name = comp_idx.fields.join("_");
                let new_tuple_values: Vec<String> = comp_idx
                    .fields
                    .iter()
                    .map(|fname| {
                        let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                        let field_name = self.get_field_param_name(field);
                        format!("self.records[idx].{}.clone()", field_name)
                    })
                    .collect();
                let new_tuple = format!("({})", new_tuple_values.join(", "));
                code.push_str(&format!(
                    "        self.{}_index.entry({}).or_insert_with(Vec::new).push(idx);\n",
                    index_name, new_tuple
                ));
            }

            code.push_str("        Ok(self.records[idx].clone())\n");
            code.push_str("    }\n\n");
        }
    }

    fn generate_delete_method(&self, code: &mut String, model: &Model) {
        let id_field = model.fields.iter().find(|f| f.auto_generate);

        if let Some(id_field) = id_field {
            code.push_str(&format!(
                "    pub fn delete(&mut self, {}: {}) -> Result<(), String> {{\n",
                id_field.name,
                id_field.field_type.to_rust_type()
            ));

            // Find the record
            code.push_str("        let idx = self.records.iter().enumerate()\n");
            code.push_str(&format!(
                "            .find(|(i, r)| !self.tombstones[*i] && r.{} == {})\n",
                id_field.name, id_field.name
            ));
            code.push_str("            .map(|(i, _)| i)\n");
            code.push_str("            .ok_or_else(|| \"Record not found\".to_string())?;\n\n");

            // Mark as deleted (tombstone)
            code.push_str("        self.tombstones[idx] = true;\n\n");

            // Remove from indexes (optional optimization to free memory)
            for field in &model.fields {
                if Self::is_virtual_field(field) {
                    continue;
                }

                let param_name = self.get_field_param_name(field);
                let is_fk =
                    matches!(&field.field_type, FieldType::Relation(rel) if rel.is_reference());

                if field.unique {
                    code.push_str(&format!(
                        "        self.{}_index.remove(&self.records[idx].{});\n",
                        param_name, param_name
                    ));
                } else if field.indexed || is_fk {
                    match field.index_type {
                        IndexType::Hash => {
                            code.push_str(&format!("        if let Some(indices) = self.{}_index.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                            code.push_str("            indices.retain(|&i| i != idx);\n");
                            code.push_str("        }\n");
                        }
                        IndexType::BTree => {
                            if matches!(field.field_type, FieldType::F64) {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get_mut(&ordered_float::OrderedFloat(self.records[idx].{})) {{\n", param_name, param_name));
                            } else {
                                code.push_str(&format!("        if let Some(indices) = self.{}_btree.get_mut(&self.records[idx].{}) {{\n", param_name, param_name));
                            }
                            code.push_str("            indices.retain(|&i| i != idx);\n");
                            code.push_str("        }\n");
                        }
                    }
                }
            }

            // Remove from composite indexes
            for comp_idx in &model.composite_indexes {
                let index_name = comp_idx.fields.join("_");
                let tuple_values: Vec<String> = comp_idx
                    .fields
                    .iter()
                    .map(|fname| {
                        let field = model.fields.iter().find(|f| &f.name == fname).unwrap();
                        let field_name = self.get_field_param_name(field);
                        format!("self.records[idx].{}.clone()", field_name)
                    })
                    .collect();
                let tuple = format!("({})", tuple_values.join(", "));
                code.push_str(&format!(
                    "        if let Some(indices) = self.{}_index.get_mut(&{}) {{\n",
                    index_name, tuple
                ));
                code.push_str("            indices.retain(|&i| i != idx);\n");
                code.push_str("        }\n");
            }

            code.push_str("        Ok(())\n");
            code.push_str("    }\n\n");
        }
    }

    /// Generate helper methods for computed fields (Sprint 12)
    fn generate_computed_accessors(&self, code: &mut String, model: &Model) {
        let computed_fields: Vec<&Field> = model.fields.iter().filter(|f| f.is_computed).collect();

        if computed_fields.is_empty() {
            return;
        }

        // Generate a method to get a record with computed fields
        // This method takes a trait implementation as a generic parameter
        let id_field = model.fields.iter().find(|f| f.auto_generate);
        if let Some(id_field) = id_field {
            code.push_str(&format!("    /// Get a record with its computed fields\n"));
            code.push_str(&format!(
                "    pub fn get_with_computed<C: {}Computed>(&self, {}: {}) -> Option<{}> {{\n",
                model.name,
                id_field.name,
                id_field.field_type.to_rust_type(),
                model.name
            ));

            code.push_str(&format!("        self.get({})\n", id_field.name));
            code.push_str("    }\n\n");

            // Generate a method to compute a specific field value
            for field in &computed_fields {
                code.push_str(&format!(
                    "    /// Compute the value of '{}' for a record\n",
                    field.name
                ));
                code.push_str(&format!(
                    "    pub fn compute_{}<C: {}Computed>(&self, {}: {}) -> Option<{}> {{\n",
                    field.name,
                    model.name,
                    id_field.name,
                    id_field.field_type.to_rust_type(),
                    field.field_type.to_rust_type()
                ));

                code.push_str(&format!(
                    "        self.get({}).map(|record| C::{}(&record))\n",
                    id_field.name, field.name
                ));
                code.push_str("    }\n\n");
            }
        }
    }

    fn generate_database_struct(&self, schema: &Schema) -> String {
        let mut code = String::new();

        code.push_str("pub struct Database {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "    pub {}: {}Storage,\n",
                model.name.to_lowercase(),
                model.name
            ));
        }
        code.push_str("}\n\n");

        code.push_str("impl Database {\n");
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str("        Database {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "            {}: {}Storage::new(),\n",
                model.name.to_lowercase(),
                model.name
            ));
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

    fn generate_db_insert_with_fk_validation(
        &self,
        code: &mut String,
        model: &Model,
        _schema: &Schema,
    ) {
        // Find FK fields in this model
        let fk_fields: Vec<&Field> = model
            .fields
            .iter()
            .filter(|f| matches!(&f.field_type, FieldType::Relation(rel) if rel.is_reference()))
            .collect();

        if fk_fields.is_empty() {
            return; // No FKs to validate
        }

        let method_name = format!("insert_{}", model.name.to_lowercase());
        code.push_str(&format!("    pub fn {}(&mut self", method_name));

        // Parameters: all fields except auto-generated and virtual
        for field in &model.fields {
            if Self::is_virtual_field(field) {
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
                        code.push_str(&format!(
                            "        if self.{}.get({}).is_none() {{\n",
                            storage_name, fk_param
                        ));
                        code.push_str(&format!("            return Err(\"Foreign key validation failed: {} does not exist\".to_string());\n", target_model));
                        code.push_str("        }\n");
                    }
                    RelationType::OptionalReference(_) => {
                        code.push_str(&format!("        if let Some(fk) = {} {{\n", fk_param));
                        code.push_str(&format!(
                            "            if self.{}.get(fk).is_none() {{\n",
                            storage_name
                        ));
                        code.push_str(&format!("                return Err(\"Foreign key validation failed: {} does not exist\".to_string());\n", target_model));
                        code.push_str("            }\n");
                        code.push_str("        }\n");
                    }
                    _ => {}
                }
            }
        }

        // Call the underlying storage insert
        code.push_str(&format!(
            "        self.{}.insert(",
            model.name.to_lowercase()
        ));
        let mut first = true;
        for field in &model.fields {
            if Self::is_virtual_field(field) {
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

    fn generate_relation_traversal_method(
        &self,
        code: &mut String,
        relation: &crate::ast::RelationPair,
        schema: &Schema,
    ) {
        // Generate parent.children() method
        // e.g., user.posts() -> Vec<Post>
        let parent_storage = relation.parent_model.to_lowercase();
        let child_storage = relation.child_model.to_lowercase();
        let method_name = format!("{}_{}", parent_storage, relation.parent_field); // e.g., user_posts

        let parent_model = schema.find_model(&relation.parent_model).unwrap();
        let id_field = parent_model
            .fields
            .iter()
            .find(|f| f.auto_generate)
            .unwrap();

        code.push_str(&format!(
            "    pub fn {}(&self, {}_id: {}) -> Vec<{}> {{\n",
            method_name,
            parent_storage,
            id_field.field_type.to_rust_type(),
            relation.child_model
        ));

        code.push_str(&format!(
            "        self.{}.find_by_{}_id({}_id)\n",
            child_storage, parent_storage, parent_storage
        ));

        code.push_str("    }\n\n");
    }

    fn generate_reverse_lookup_method(
        &self,
        code: &mut String,
        relation: &crate::ast::RelationPair,
        schema: &Schema,
    ) {
        // Generate child.parent() method
        // e.g., post.author() -> Option<User>
        let parent_storage = relation.parent_model.to_lowercase();
        let child_storage = relation.child_model.to_lowercase();
        let method_name = format!("{}_{}", child_storage, relation.child_field); // e.g., post_author

        let child_model = schema.find_model(&relation.child_model).unwrap();
        let id_field = child_model.fields.iter().find(|f| f.auto_generate).unwrap();

        let return_type = if relation.is_required {
            format!("Option<{}>", relation.parent_model)
        } else {
            format!("Option<{}>", relation.parent_model)
        };

        code.push_str(&format!(
            "    pub fn {}(&self, {}_id: {}) -> {} {{\n",
            method_name,
            child_storage,
            id_field.field_type.to_rust_type(),
            return_type
        ));

        code.push_str(&format!(
            "        if let Some(child) = self.{}.get({}_id) {{\n",
            child_storage, child_storage
        ));

        code.push_str(&format!(
            "            return self.{}.get(child.{}_id);\n",
            parent_storage, relation.child_field
        ));

        code.push_str("        }\n");
        code.push_str("        None\n");
        code.push_str("    }\n\n");
    }

    /// Generate junction table name from two models
    fn junction_table_name(m2m: &ManyToManyRelation) -> String {
        // Sort model names alphabetically for consistency
        let (model1, model2, _field1, _field2) = if m2m.model1 < m2m.model2 {
            (&m2m.model1, &m2m.model2, &m2m.field1, &m2m.field2)
        } else {
            (&m2m.model2, &m2m.model1, &m2m.field2, &m2m.field1)
        };

        // If there's only one M:N between these models, use simple name
        // Otherwise include field name to differentiate
        format!("{}{}Junction", model1, model2)
    }

    /// Generate junction table storage struct
    fn generate_junction_table(&self, m2m: &ManyToManyRelation, schema: &Schema) -> String {
        let mut code = String::new();
        let junction_name = Self::junction_table_name(m2m);

        // Find ID types for both models
        let model1 = schema.find_model(&m2m.model1).unwrap();
        let model2 = schema.find_model(&m2m.model2).unwrap();
        let id1_field = model1.fields.iter().find(|f| f.auto_generate).unwrap();
        let id2_field = model2.fields.iter().find(|f| f.auto_generate).unwrap();
        let id1_type = id1_field.field_type.to_rust_type();
        let id2_type = id2_field.field_type.to_rust_type();

        // Junction record struct
        code.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\n"));
        code.push_str(&format!("pub struct {} {{\n", junction_name));
        code.push_str(&format!(
            "    pub {}_id: {},\n",
            m2m.model1.to_lowercase(),
            id1_type
        ));
        code.push_str(&format!(
            "    pub {}_id: {},\n",
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str("}\n\n");

        // Junction storage struct
        code.push_str(&format!("pub struct {}Storage {{\n", junction_name));
        code.push_str(&format!("    records: Vec<{}>,\n", junction_name));
        code.push_str(&format!(
            "    {}_to_{}_index: std::collections::HashMap<{}, Vec<{}>>,\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            id1_type,
            id2_type
        ));
        code.push_str(&format!(
            "    {}_to_{}_index: std::collections::HashMap<{}, Vec<{}>>,\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            id2_type,
            id1_type
        ));
        code.push_str("}\n\n");

        // Implementation
        code.push_str(&format!("impl {}Storage {{\n", junction_name));

        // new()
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str(&format!("        {}Storage {{\n", junction_name));
        code.push_str("            records: Vec::new(),\n");
        code.push_str(&format!(
            "            {}_to_{}_index: std::collections::HashMap::new(),\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "            {}_to_{}_index: std::collections::HashMap::new(),\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // add_relation()
        code.push_str(&format!(
            "    pub fn add_relation(&mut self, {}_id: {}, {}_id: {}) {{\n",
            m2m.model1.to_lowercase(),
            id1_type,
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str(&format!("        // Check if relation already exists\n"));
        code.push_str(&format!(
            "        if self.has_relation({}_id, {}_id) {{\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str("            return;\n");
        code.push_str("        }\n\n");
        code.push_str(&format!("        let record = {} {{\n", junction_name));
        code.push_str(&format!("            {}_id,\n", m2m.model1.to_lowercase()));
        code.push_str(&format!("            {}_id,\n", m2m.model2.to_lowercase()));
        code.push_str("        };\n");
        code.push_str("        self.records.push(record);\n");
        code.push_str(&format!(
            "        self.{}_to_{}_index.entry({}_id).or_insert_with(Vec::new).push({}_id);\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "        self.{}_to_{}_index.entry({}_id).or_insert_with(Vec::new).push({}_id);\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str("    }\n\n");

        // remove_relation()
        code.push_str(&format!(
            "    pub fn remove_relation(&mut self, {}_id: {}, {}_id: {}) {{\n",
            m2m.model1.to_lowercase(),
            id1_type,
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str(&format!(
            "        self.records.retain(|r| !(r.{}_id == {}_id && r.{}_id == {}_id));\n",
            m2m.model1.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "        if let Some(ids) = self.{}_to_{}_index.get_mut(&{}_id) {{\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str(&format!(
            "            ids.retain(|&id| id != {}_id);\n",
            m2m.model2.to_lowercase()
        ));
        code.push_str("        }\n");
        code.push_str(&format!(
            "        if let Some(ids) = self.{}_to_{}_index.get_mut(&{}_id) {{\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "            ids.retain(|&id| id != {}_id);\n",
            m2m.model1.to_lowercase()
        ));
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // get_related_ids() for model1 -> model2
        code.push_str(&format!(
            "    pub fn get_{}_{}(&self, {}_id: {}) -> Vec<{}> {{\n",
            m2m.model1.to_lowercase(),
            m2m.field1,
            m2m.model1.to_lowercase(),
            id1_type,
            id2_type
        ));
        code.push_str(&format!(
            "        self.{}_to_{}_index.get(&{}_id)\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str("            .map(|ids| ids.clone())\n");
        code.push_str("            .unwrap_or_else(Vec::new)\n");
        code.push_str("    }\n\n");

        // get_related_ids() for model2 -> model1
        code.push_str(&format!(
            "    pub fn get_{}_{}(&self, {}_id: {}) -> Vec<{}> {{\n",
            m2m.model2.to_lowercase(),
            m2m.field2,
            m2m.model2.to_lowercase(),
            id2_type,
            id1_type
        ));
        code.push_str(&format!(
            "        self.{}_to_{}_index.get(&{}_id)\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str("            .map(|ids| ids.clone())\n");
        code.push_str("            .unwrap_or_else(Vec::new)\n");
        code.push_str("    }\n\n");

        // has_relation()
        code.push_str(&format!(
            "    pub fn has_relation(&self, {}_id: {}, {}_id: {}) -> bool {{\n",
            m2m.model1.to_lowercase(),
            id1_type,
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str(&format!(
            "        self.records.iter().any(|r| r.{}_id == {}_id && r.{}_id == {}_id)\n",
            m2m.model1.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str("    }\n");

        code.push_str("}\n\n");

        code
    }

    /// Generate multi-file output
    pub fn generate_files(&self, schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = Vec::new();

        // Standard imports that all files need
        let common_imports = "use std::collections::HashMap;\nuse std::time::{SystemTime, UNIX_EPOCH};\nuse uuid::Uuid;\n";

        // Check if any model uses constraints - if so, add regex import
        let has_constraints = schema
            .models
            .iter()
            .any(|m| m.fields.iter().any(|f| !f.constraints.is_empty()));
        let constraint_imports = if has_constraints { "use regex;\n" } else { "" };

        // Generate validation functions if needed
        let validation_funcs = if has_constraints {
            self.generate_validation_functions()
        } else {
            String::new()
        };

        // Generate struct definitions file (Sprint 8)
        if !schema.structs.is_empty() {
            let mut struct_content = String::new();
            struct_content.push_str("// Generated code - do not edit manually\n\n");
            struct_content.push_str("// Struct definitions\n\n");
            for struct_def in &schema.structs {
                struct_content.push_str(&self.generate_struct_definition(struct_def));
            }
            files.push(GeneratedFile {
                path: "structs.rs".to_string(),
                content: struct_content,
            });
        }

        // Generate one file per model
        for model in &schema.models {
            let mut content = String::new();
            content.push_str("// Generated code - do not edit manually\n\n");
            content.push_str(common_imports);
            content.push_str(constraint_imports);
            content.push_str("\n");

            // Import structs if this model uses them
            if !schema.structs.is_empty() {
                let uses_structs = model
                    .fields
                    .iter()
                    .any(|f| f.field_type.struct_name().is_some());
                if uses_structs {
                    content.push_str("use super::structs::*;\n\n");
                }
            }

            if has_constraints && !validation_funcs.is_empty() {
                content.push_str(&validation_funcs);
            }

            content.push_str(&self.generate_struct(model));
            content.push_str(&self.generate_computed_trait(model));
            content.push_str(&self.generate_storage_struct(model));
            content.push_str(&self.generate_storage_impl(model));

            files.push(GeneratedFile {
                path: format!("{}_storage.rs", model.name.to_lowercase()),
                content,
            });
        }

        // Generate junction tables for M:N relations
        let m2m_relations = schema.detect_many_to_many_relations();
        for m2m in &m2m_relations {
            let mut content = String::new();
            content.push_str("// Generated code - do not edit manually\n\n");
            content.push_str(common_imports);
            content.push_str("\n");

            content.push_str(&self.generate_junction_table(m2m, schema));

            let junction_name = Self::junction_table_name(m2m);
            files.push(GeneratedFile {
                path: format!("{}_junction.rs", junction_name.to_lowercase()),
                content,
            });
        }

        // Generate mod.rs
        let mut mod_content = String::new();
        mod_content.push_str("// Generated code - do not edit manually\n\n");

        // Export structs module if it exists
        if !schema.structs.is_empty() {
            mod_content.push_str("pub mod structs;\n");
            mod_content.push_str("pub use structs::*;\n\n");
        }

        for model in &schema.models {
            mod_content.push_str(&format!("mod {}_storage;\n", model.name.to_lowercase()));
            mod_content.push_str(&format!(
                "pub use {}_storage::*;\n\n",
                model.name.to_lowercase()
            ));
        }

        for m2m in &m2m_relations {
            let junction_name = Self::junction_table_name(m2m);
            mod_content.push_str(&format!("mod {}_junction;\n", junction_name.to_lowercase()));
            mod_content.push_str(&format!(
                "pub use {}_junction::*;\n\n",
                junction_name.to_lowercase()
            ));
        }

        files.push(GeneratedFile {
            path: "mod.rs".to_string(),
            content: mod_content,
        });

        // Generate database.rs
        let mut db_content = String::new();
        db_content.push_str("// Generated code - do not edit manually\n\n");
        db_content.push_str("use super::*;\n\n");

        db_content.push_str(&self.generate_database_struct_multifile(schema, &m2m_relations));

        files.push(GeneratedFile {
            path: "database.rs".to_string(),
            content: db_content,
        });

        files
    }

    fn generate_database_struct_multifile(
        &self,
        schema: &Schema,
        m2m_relations: &[ManyToManyRelation],
    ) -> String {
        let mut code = String::new();

        code.push_str("pub struct Database {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "    pub {}: {}Storage,\n",
                model.name.to_lowercase(),
                model.name
            ));
        }

        // Add junction table storages with unique field names
        for m2m in m2m_relations {
            let junction_name = Self::junction_table_name(m2m);
            // Use field name from model1's perspective for the junction field
            let field_name = format!("{}_{}", m2m.model1.to_lowercase(), m2m.field1);
            code.push_str(&format!(
                "    pub {}: {}Storage,\n",
                field_name, junction_name
            ));
        }
        code.push_str("}\n\n");

        code.push_str("impl Database {\n");
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str("        Database {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "            {}: {}Storage::new(),\n",
                model.name.to_lowercase(),
                model.name
            ));
        }
        for m2m in m2m_relations {
            let junction_name = Self::junction_table_name(m2m);
            let field_name = format!("{}_{}", m2m.model1.to_lowercase(), m2m.field1);
            code.push_str(&format!(
                "            {}: {}Storage::new(),\n",
                field_name, junction_name
            ));
        }
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // Generate FK validation insert methods
        for model in &schema.models {
            self.generate_db_insert_with_fk_validation(&mut code, model, schema);
        }

        // Generate relation traversal methods (OneToMany only, not M:N)
        let relations = schema.detect_relations();
        for relation in &relations {
            self.generate_relation_traversal_method(&mut code, relation, schema);
            self.generate_reverse_lookup_method(&mut code, relation, schema);
        }

        code.push_str("}\n\n");

        code
    }

    pub fn generate(&self, schema: &Schema) -> String {
        let mut code = String::new();

        // Add standard imports
        code.push_str("// Generated code - do not edit manually\n\n");
        code.push_str("use std::collections::HashMap;\n");
        code.push_str("use std::time::{SystemTime, UNIX_EPOCH};\n");
        code.push_str("use uuid::Uuid;\n");

        // Generate struct definitions (Sprint 8)
        if !schema.structs.is_empty() {
            code.push_str("\n// Struct Definitions\n\n");
            for struct_def in &schema.structs {
                code.push_str(&self.generate_struct_definition(struct_def));
            }
        }

        // Check if any model uses constraints - if so, add regex import
        let has_constraints = schema
            .models
            .iter()
            .any(|m| m.fields.iter().any(|f| !f.constraints.is_empty()));

        if has_constraints {
            code.push_str("use regex;\n");
        }

        code.push_str("\n");

        // Generate validation functions if any constraints exist
        if has_constraints {
            code.push_str(&self.generate_validation_functions());
        }

        // Generate code for each model
        for model in &schema.models {
            code.push_str(&self.generate_struct(model));
            code.push_str(&self.generate_computed_trait(model));
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
        assert!(code.contains(
            "if self.email_index.contains_key(&email) && self.records[idx].email != email"
        ));
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

    #[test]
    fn test_generate_constraint_email() {
        let input = r#"
User {
  id: +uuid
  email: string @email
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check that regex is imported
        assert!(code.contains("use regex;"));

        // Check that validation function is generated
        assert!(code.contains("fn validate_email(value: &str) -> Result<(), String>"));
        assert!(code.contains("email_regex"));

        // Check that validation is called in insert
        assert!(code.contains("validate_email(&email)?"));
    }

    #[test]
    fn test_generate_constraint_min_max() {
        let input = r#"
User {
  id: +uuid
  age: u32 @min(0) @max(150)
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check that min validation is generated
        assert!(code.contains("if age < 0"));
        assert!(code.contains("age must be at least 0"));

        // Check that max validation is generated
        assert!(code.contains("if age > 150"));
        assert!(code.contains("age must be at most 150"));
    }

    #[test]
    fn test_generate_constraint_string_length() {
        let input = r#"
User {
  id: +uuid
  password: string @min(8) @max(100)
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check that string length validation is generated
        assert!(code.contains("if password.len() < 8"));
        assert!(code.contains("password must be at least 8 characters"));

        assert!(code.contains("if password.len() > 100"));
        assert!(code.contains("password must be at most 100 characters"));
    }

    #[test]
    fn test_generate_constraint_url() {
        let input = r#"
User {
  id: +uuid
  website: string @url
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check that URL validation function is generated
        assert!(code.contains("fn validate_url(value: &str) -> Result<(), String>"));

        // Check that validation is called
        assert!(code.contains("validate_url(&website)?"));
    }

    #[test]
    fn test_generate_multiple_constraints() {
        let input = r#"
User {
  id: +uuid
  email: string @email
  age: u32 @min(0) @max(150)
  website: string @url
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check all validations are present
        assert!(code.contains("validate_email(&email)?"));
        assert!(code.contains("if age < 0"));
        assert!(code.contains("if age > 150"));
        assert!(code.contains("validate_url(&website)?"));
    }

    #[test]
    fn test_generate_no_regex_import_without_constraints() {
        let input = r#"
User {
  id: +uuid
  name: string
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Should NOT include regex import when no constraints
        assert!(!code.contains("use regex;"));
    }

    #[test]
    fn test_generate_constraint_validation_order() {
        let input = r#"
User {
  id: +uuid
  email: ^&string @email
  age: u32 @min(13) @max(120)
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Validation should happen BEFORE unique constraint checks
        let validation_pos = code.find("validate_email(&email)?").unwrap();
        let unique_check_pos = code.find("if self.email_index.contains_key").unwrap();

        assert!(
            validation_pos < unique_check_pos,
            "Validation should come before unique constraint checks"
        );
    }

    #[test]
    fn test_generate_constraint_only_on_non_autogen_fields() {
        let input = r#"
User {
  id: +uuid @email
  email: string @email
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Auto-generated fields should not have validation (uuid doesn't need email validation)
        // Only the email field should have validation
        let email_validation_count = code.matches("validate_email").count();

        // Should only validate the non-auto-generated email field
        assert_eq!(email_validation_count, 2); // 1 function definition + 1 call
    }

    #[test]
    fn test_generate_constraint_boundary_values() {
        let input = r#"
User {
  id: +uuid
  age: u32 @min(0) @max(255)
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check exact boundary values are preserved
        assert!(code.contains("if age < 0"));
        assert!(code.contains("if age > 255"));
        assert!(code.contains("age must be at least 0"));
        assert!(code.contains("age must be at most 255"));
    }

    #[test]
    fn test_generate_validation_error_messages() {
        let input = r#"
User {
  id: +uuid
  username: string @min(3)
  age: u32 @min(13)
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Check descriptive error messages are generated
        assert!(code.contains("username must be at least 3 characters"));
        assert!(code.contains("age must be at least 13"));
    }

    #[test]
    fn test_generate_constraints_skip_relations() {
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

        // Relations should not have validation code generated
        // Verify the code compiles without constraint-related errors for relations
        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub struct Post"));
    }

    #[test]
    fn test_generate_mixed_constraints_and_symbols() {
        // Test that constraints work with other field modifiers
        let input = r#"
User {
  id: +uuid
  email: ^&string @email
}
"#;
        let mut parser = Parser::new(input).unwrap();
        let schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&schema);

        // Should handle field with indexed, unique, AND constraint
        assert!(code.contains("pub email: String"));
        assert!(code.contains("email_index"));
        assert!(code.contains("validate_email(&email)?"));
    }
}
