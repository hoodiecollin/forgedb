use super::utils::is_virtual_field;
use crate::ast::{FieldType, IndexType, Model, RelationType};

pub struct StorageGenerator;

impl StorageGenerator {
    pub fn new() -> Self {
        StorageGenerator
    }

    pub fn generate_storage_struct(&self, model: &Model) -> String {
        let mut code = String::new();

        code.push_str(&format!("pub struct {}Storage {{\n", model.name));
        code.push_str(&format!("    records: Vec<{}>,\n", model.name));
        code.push_str("    next_id: u64,\n");
        code.push_str("    tombstones: Vec<bool>,\n");

        // Add index maps for fields with ^ or & symbols, and for FK fields
        for field in &model.fields {
            // Skip virtual fields
            if is_virtual_field(field) {
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
                    code.push_str(&format!("    {}_fulltext: std::sync::Arc<std::sync::RwLock<forgedb_fulltext::FullTextIndex>>,\n",
                        field.name));
                }
            }
        }

        // Add materialized computed field storage (Sprint 19)
        let has_materialized = model.fields.iter().any(|f| f.is_materialized);
        if has_materialized {
            code.push_str("    // Materialized computed fields\n");
            for field in &model.fields {
                if field.is_materialized {
                    code.push_str(&format!(
                        "    materialized_{}: std::collections::HashMap<uuid::Uuid, {}>,\n",
                        field.name,
                        field.field_type.to_rust_type()
                    ));
                }
            }
        }

        // Add soft delete tracking (Sprint 19)
        if model.soft_delete {
            code.push_str("    // Soft delete tracking\n");
            code.push_str("    deleted_at: std::collections::HashMap<uuid::Uuid, i64>,\n");
        }

        code.push_str("}\n\n");

        code
    }

    pub fn generate_storage_impl(&self, model: &Model) -> String {
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
            if is_virtual_field(field) {
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
                code.push_str(&format!("            {}_fulltext: std::sync::Arc::new(std::sync::RwLock::new(forgedb_fulltext::FullTextIndex::new())),\n",
                    field.name));
            }
        }

        // Initialize materialized fields (Sprint 19)
        for field in &model.fields {
            if field.is_materialized {
                code.push_str(&format!(
                    "            materialized_{}: std::collections::HashMap::new(),\n",
                    field.name
                ));
            }
        }

        // Initialize soft delete tracking (Sprint 19)
        if model.soft_delete {
            code.push_str("            deleted_at: std::collections::HashMap::new(),\n");
        }

        code.push_str("        }\n");
        code.push_str("    }\n");

        code.push_str("}\n\n");

        code
    }
}
