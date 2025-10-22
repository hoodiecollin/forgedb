//! OpenAPI specification generator

use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;

/// OpenAPI specification format
#[derive(Debug, Clone, Copy)]
pub enum OpenApiFormat {
    /// YAML format
    Yaml,
    /// JSON format
    Json,
}

/// OpenAPI generator
pub struct OpenApiGenerator;

impl OpenApiGenerator {
    /// Generate OpenAPI specification from schema
    ///
    /// # Arguments
    ///
    /// * `schema` - Parsed schema AST
    /// * `format` - Output format (YAML or JSON)
    ///
    /// # Returns
    ///
    /// Generated OpenAPI specification as a string
    pub fn generate(schema: &Schema, format: OpenApiFormat) -> Result<GeneratedCode> {
        let code = Self::generate_spec(schema, format)?;

        Ok(GeneratedCode {
            code,
            description: format!(
                "OpenAPI {} specification ({} models)",
                match format {
                    OpenApiFormat::Yaml => "YAML",
                    OpenApiFormat::Json => "JSON",
                },
                schema.models.len()
            ),
        })
    }

    /// Generate OpenAPI specification as a string
    fn generate_spec(schema: &Schema, format: OpenApiFormat) -> Result<String> {
        match format {
            OpenApiFormat::Yaml => Self::generate_yaml(schema),
            OpenApiFormat::Json => Self::generate_json(schema),
        }
    }

    /// Generate YAML format
    fn generate_yaml(schema: &Schema) -> Result<String> {
        let mut yaml = String::new();

        yaml.push_str("openapi: 3.0.0\n");
        yaml.push_str("info:\n");
        yaml.push_str("  title: ForgeDB API\n");
        yaml.push_str("  version: 1.0.0\n");
        yaml.push_str("  description: Auto-generated API from ForgeDB schema\n");
        yaml.push_str("\n");
        yaml.push_str("servers:\n");
        yaml.push_str("  - url: http://localhost:3000/api\n");
        yaml.push_str("    description: Development server\n");
        yaml.push_str("\n");
        yaml.push_str("paths:\n");

        for model in &schema.models {
            let path = format!("/{}", Self::to_kebab_case(&model.name));

            yaml.push_str(&format!("  {}:\n", path));
            yaml.push_str("    get:\n");
            yaml.push_str(&format!("      summary: List {}\n", model.name));
            yaml.push_str(&format!("      operationId: list{}\n", model.name));
            yaml.push_str(&format!("      tags: [{}]\n", model.name));
            yaml.push_str("      responses:\n");
            yaml.push_str("        '200':\n");
            yaml.push_str("          description: Success\n");
            yaml.push_str("          content:\n");
            yaml.push_str("            application/json:\n");
            yaml.push_str("              schema:\n");
            yaml.push_str("                type: object\n");
            yaml.push_str("                properties:\n");
            yaml.push_str("                  data:\n");
            yaml.push_str("                    type: array\n");
            yaml.push_str("                    items:\n");
            yaml.push_str(&format!("                      $ref: '#/components/schemas/{}'\n", model.name));
            yaml.push_str("    post:\n");
            yaml.push_str(&format!("      summary: Create {}\n", model.name));
            yaml.push_str(&format!("      operationId: create{}\n", model.name));
            yaml.push_str(&format!("      tags: [{}]\n", model.name));
            yaml.push_str("      responses:\n");
            yaml.push_str("        '201':\n");
            yaml.push_str("          description: Created\n");
            yaml.push_str("\n");

            yaml.push_str(&format!("  {}/:id:\n", path));
            yaml.push_str("    get:\n");
            yaml.push_str(&format!("      summary: Get {} by ID\n", model.name));
            yaml.push_str(&format!("      operationId: get{}\n", model.name));
            yaml.push_str(&format!("      tags: [{}]\n", model.name));
            yaml.push_str("      parameters:\n");
            yaml.push_str("        - name: id\n");
            yaml.push_str("          in: path\n");
            yaml.push_str("          required: true\n");
            yaml.push_str("          schema:\n");
            yaml.push_str("            type: string\n");
            yaml.push_str("      responses:\n");
            yaml.push_str("        '200':\n");
            yaml.push_str("          description: Success\n");
            yaml.push_str("        '404':\n");
            yaml.push_str("          description: Not found\n");
            yaml.push_str("\n");
        }

        yaml.push_str("components:\n");
        yaml.push_str("  schemas:\n");

        for model in &schema.models {
            yaml.push_str(&format!("    {}:\n", model.name));
            yaml.push_str("      type: object\n");
            yaml.push_str("      properties:\n");

            for field in &model.fields {
                yaml.push_str(&format!("        {}:\n", field.name));
                yaml.push_str(&format!("          type: {}\n", Self::map_openapi_type(&field.field_type)));
                if field.is_nullable() {
                    yaml.push_str("          nullable: true\n");
                }
            }

            yaml.push_str("\n");
        }

        Ok(yaml)
    }

    /// Generate JSON format
    fn generate_json(schema: &Schema) -> Result<String> {
        // For now, just wrap YAML in a simple JSON structure
        // A full implementation would use serde_json
        let yaml = Self::generate_yaml(schema)?;
        Ok(format!("{{\"openapi\": \"3.0.0\", \"info\": {{\"title\": \"ForgeDB API\", \"version\": \"1.0.0\"}}, \"note\": \"Full JSON implementation pending\", \"yamlVersion\": {:?}}}", yaml))
    }

    /// Map ForgeDB field type to OpenAPI type
    fn map_openapi_type(field_type: &forgedb_parser::FieldType) -> &'static str {
        match field_type {
            forgedb_parser::FieldType::I32 | forgedb_parser::FieldType::I64 => "integer",
            forgedb_parser::FieldType::F64 => "number",
            forgedb_parser::FieldType::Bool => "boolean",
            forgedb_parser::FieldType::String | forgedb_parser::FieldType::Uuid => "string",
            forgedb_parser::FieldType::Timestamp => "integer",
            _ => "string",
        }
    }

    /// Convert PascalCase to kebab-case
    fn to_kebab_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('-');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }
}
