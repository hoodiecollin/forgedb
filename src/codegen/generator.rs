use crate::ast::{ManyToManyRelation, Model, Schema, Struct};

use super::computed::{ComputedAccessorGenerator, ComputedTraitGenerator};
use super::crud::{DeleteGenerator, GetGenerator, InsertGenerator, UpdateGenerator};
use super::model_gen::ModelGenerator;
use super::output::{MultiFileGenerator, SingleFileGenerator};
use super::query::{FindByGenerator, ListGenerator, SearchGenerator};
use super::relations::{ForeignKeyGenerator, JunctionTableGenerator, RelationTraversalGenerator};
use super::storage_gen::StorageGenerator;
use super::validation_gen::ValidationGenerator;
use super::GeneratedFile;

pub struct CodeGenerator {
    model_gen: ModelGenerator,
    validation_gen: ValidationGenerator,
    storage_gen: StorageGenerator,
    insert_gen: InsertGenerator,
    get_gen: GetGenerator,
    update_gen: UpdateGenerator,
    delete_gen: DeleteGenerator,
    find_by_gen: FindByGenerator,
    list_gen: ListGenerator,
    search_gen: SearchGenerator,
    computed_trait_gen: ComputedTraitGenerator,
    computed_accessor_gen: ComputedAccessorGenerator,
    fk_gen: ForeignKeyGenerator,
    traversal_gen: RelationTraversalGenerator,
    junction_gen: JunctionTableGenerator,
    single_file_gen: SingleFileGenerator,
    multi_file_gen: MultiFileGenerator,
}

impl CodeGenerator {
    pub fn new() -> Self {
        CodeGenerator {
            model_gen: ModelGenerator::new(),
            validation_gen: ValidationGenerator::new(),
            storage_gen: StorageGenerator::new(),
            insert_gen: InsertGenerator::new(),
            get_gen: GetGenerator::new(),
            update_gen: UpdateGenerator::new(),
            delete_gen: DeleteGenerator::new(),
            find_by_gen: FindByGenerator::new(),
            list_gen: ListGenerator::new(),
            search_gen: SearchGenerator::new(),
            computed_trait_gen: ComputedTraitGenerator::new(),
            computed_accessor_gen: ComputedAccessorGenerator::new(),
            fk_gen: ForeignKeyGenerator::new(),
            traversal_gen: RelationTraversalGenerator::new(),
            junction_gen: JunctionTableGenerator::new(),
            single_file_gen: SingleFileGenerator::new(),
            multi_file_gen: MultiFileGenerator::new(),
        }
    }

    pub fn generate_struct_definition(&self, struct_def: &Struct) -> String {
        self.model_gen.generate_struct_definition(struct_def)
    }

    pub fn generate_struct(&self, model: &Model) -> String {
        self.model_gen.generate_struct(model)
    }

    pub fn generate_computed_trait(&self, model: &Model) -> String {
        self.computed_trait_gen.generate_computed_trait(model)
    }

    pub fn generate_storage_struct(&self, model: &Model) -> String {
        self.storage_gen.generate_storage_struct(model)
    }

    pub fn generate_storage_impl(&self, model: &Model) -> String {
        let mut code = self.storage_gen.generate_storage_impl(model);

        // Generate CRUD methods
        self.insert_gen.generate_insert_method(&mut code, model);
        self.get_gen.generate_get_method(&mut code, model);

        // Generate query methods
        self.find_by_gen.generate_find_by_methods(&mut code, model);
        self.list_gen.generate_list_method(&mut code, model);
        self.search_gen.generate_search_methods(&mut code, model);

        // Generate update and delete
        self.update_gen.generate_update_method(&mut code, model);
        self.delete_gen.generate_delete_method(&mut code, model);

        // Generate computed accessors
        self.computed_accessor_gen
            .generate_computed_accessors(&mut code, model);

        code.push_str("}\n\n");
        code
    }

    pub fn generate_validation_functions(&self) -> String {
        self.validation_gen.generate_validation_functions()
    }

    pub fn generate_database_struct(&self, schema: &Schema) -> String {
        let mut code = self.fk_gen.generate_database_struct(schema);

        // The FK generator creates the basic Database struct
        // Now we need to add relation traversal methods before closing the impl block
        // We need to remove the last "}\n\n" and add traversal methods
        if code.ends_with("}\n\n") {
            code.truncate(code.len() - 3); // Remove "}\n\n"
        }

        // Generate relation traversal methods
        let relations = schema.detect_relations();
        for relation in &relations {
            self.traversal_gen
                .generate_relation_traversal_method(&mut code, relation, schema);
            self.traversal_gen
                .generate_reverse_lookup_method(&mut code, relation, schema);
        }

        code.push_str("}\n\n");
        code
    }

    pub fn generate_database_struct_multifile(
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
            let junction_name = JunctionTableGenerator::junction_table_name(m2m);
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
            let junction_name = JunctionTableGenerator::junction_table_name(m2m);
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
            self.fk_gen
                .generate_db_insert_with_fk_validation(&mut code, model, schema);
        }

        // Generate relation traversal methods (OneToMany only, not M:N)
        let relations = schema.detect_relations();
        for relation in &relations {
            self.traversal_gen
                .generate_relation_traversal_method(&mut code, relation, schema);
            self.traversal_gen
                .generate_reverse_lookup_method(&mut code, relation, schema);
        }

        code.push_str("}\n\n");

        code
    }

    pub fn generate_junction_table(&self, m2m: &ManyToManyRelation, schema: &Schema) -> String {
        self.junction_gen.generate_junction_table(m2m, schema)
    }

    pub fn generate(&self, schema: &Schema) -> String {
        let mut code = String::new();

        // Add header
        code.push_str(&self.single_file_gen.generate_header(schema));

        // Generate struct definitions (Sprint 8)
        if !schema.structs.is_empty() {
            code.push_str("\n// Struct Definitions\n\n");
            for struct_def in &schema.structs {
                code.push_str(&self.generate_struct_definition(struct_def));
            }
        }

        // Generate validation functions if any constraints exist
        let has_constraints = schema
            .models
            .iter()
            .any(|m| m.fields.iter().any(|f| !f.constraints.is_empty()));

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

    pub fn generate_files(&self, schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = Vec::new();

        let (common_imports, has_constraints) = self.multi_file_gen.generate_common_imports(schema);
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
            content.push_str(&common_imports);
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
            content.push_str(&common_imports);
            content.push_str("\n");

            content.push_str(&self.generate_junction_table(m2m, schema));

            let junction_name = JunctionTableGenerator::junction_table_name(m2m);
            files.push(GeneratedFile {
                path: format!("{}_junction.rs", junction_name.to_lowercase()),
                content,
            });
        }

        // Generate mod.rs
        let mod_content = self
            .multi_file_gen
            .generate_mod_file(schema, &m2m_relations);
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
}
