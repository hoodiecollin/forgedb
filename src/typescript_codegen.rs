//! TypeScript SDK Generation for Sprint 10
//!
//! Generates TypeScript client SDK from schema definitions, including:
//! - TypeScript interfaces for all models
//! - API client classes with type-safe methods
//! - Support for relations and traversal
//! - NPM package structure with bundling config

use crate::ast::{Field, FieldType, Model, RelationType, Schema};
use crate::codegen::GeneratedFile;

pub struct TypeScriptGenerator;

impl TypeScriptGenerator {
    /// Generate all TypeScript SDK files for a schema
    pub fn generate(schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = vec![];

        // Generate TypeScript types
        files.push(Self::generate_types(schema));

        // Generate API client for each model
        for model in &schema.models {
            files.push(Self::generate_api_client(model, schema));
        }

        // Generate main index file
        files.push(Self::generate_index(schema));

        // Generate package.json
        files.push(Self::generate_package_json());

        // Generate tsconfig.json
        files.push(Self::generate_tsconfig());

        // Generate bundler config (tsup)
        files.push(Self::generate_tsup_config());

        // Generate README
        files.push(Self::generate_readme(schema));

        files
    }

    /// Generate TypeScript interfaces for all models
    fn generate_types(schema: &Schema) -> GeneratedFile {
        let mut code = String::new();

        code.push_str("// Auto-generated TypeScript types\n\n");

        // Generate struct types first (Sprint 8)
        for struct_def in &schema.structs {
            code.push_str(&format!("export interface {} {{\n", struct_def.name));
            for field in &struct_def.fields {
                let ts_type = Self::map_field_type_to_ts(&field.field_type);
                code.push_str(&format!("  {}: {};\n", field.name, ts_type));
            }
            code.push_str("}\n\n");
        }

        // Generate model types
        for model in &schema.models {
            code.push_str(&format!("export interface {} {{\n", model.name));
            for field in &model.fields {
                // Skip virtual relation fields (OneToMany, ManyToMany)
                if !Self::is_virtual_field(field) {
                    let ts_type = Self::map_field_type_to_ts(&field.field_type);
                    let optional = if Self::is_optional(&field.field_type) {
                        "?"
                    } else {
                        ""
                    };
                    code.push_str(&format!("  {}{}: {};\n", field.name, optional, ts_type));
                }
            }

            // Add computed fields to model interface (Sprint 12)
            let computed_fields: Vec<_> = model.fields.iter().filter(|f| f.is_computed).collect();
            if !computed_fields.is_empty() {
                code.push_str("\n  // Computed fields\n");
                for field in computed_fields {
                    let ts_type = Self::map_field_type_to_ts(&field.field_type);
                    code.push_str(&format!("  {}: {};\n", field.name, ts_type));
                }
            }

            code.push_str("}\n\n");

            // Generate CreateRequest type (omit auto-generated and computed fields)
            code.push_str(&format!(
                "export interface Create{}Request {{\n",
                model.name
            ));
            for field in &model.fields {
                if !field.auto_generate && !Self::is_virtual_field(field) && !field.is_computed {
                    let ts_type = Self::map_field_type_to_ts(&field.field_type);
                    let optional = if Self::is_optional(&field.field_type) {
                        "?"
                    } else {
                        ""
                    };
                    code.push_str(&format!("  {}{}: {};\n", field.name, optional, ts_type));
                }
            }
            code.push_str("}\n\n");

            // Generate UpdateRequest type (all non-computed fields optional)
            code.push_str(&format!(
                "export interface Update{}Request {{\n",
                model.name
            ));
            for field in &model.fields {
                if !field.auto_generate && !Self::is_virtual_field(field) && !field.is_computed {
                    let ts_type = Self::map_field_type_to_ts(&field.field_type);
                    code.push_str(&format!("  {}?: {};\n", field.name, ts_type));
                }
            }
            code.push_str("}\n\n");
        }

        // Generate query parameter types
        code.push_str("export interface QueryParams {\n");
        code.push_str("  limit?: number;\n");
        code.push_str("  offset?: number;\n");
        code.push_str("  sort?: string;\n");
        code.push_str("  order?: 'asc' | 'desc';\n");
        code.push_str("  [key: string]: any; // Filter params\n");
        code.push_str("}\n\n");

        code.push_str("export interface ListResponse<T> {\n");
        code.push_str("  data: T[];\n");
        code.push_str("  count: number;\n");
        code.push_str("}\n");

        GeneratedFile {
            path: "generated/sdk/types.ts".to_string(),
            content: code,
        }
    }

    /// Generate API client for a model
    fn generate_api_client(model: &Model, schema: &Schema) -> GeneratedFile {
        let model_lower = model.name.to_lowercase();
        let plural = format!("{}s", model_lower); // Simple pluralization
        let mut code = String::new();

        // Imports
        code.push_str(&format!("import type {{ {}, Create{}Request, Update{}Request, QueryParams, ListResponse }} from './types';\n\n",
            model.name, model.name, model.name));

        // API Client class
        code.push_str(&format!("export class {}Api {{\n", model.name));
        code.push_str("  private baseUrl: string;\n\n");

        // Constructor
        code.push_str("  constructor(baseUrl: string) {\n");
        code.push_str("    this.baseUrl = baseUrl;\n");
        code.push_str("  }\n\n");

        // List method
        code.push_str("  /**\n");
        code.push_str(&format!(
            "   * List all {} with optional filtering, sorting, and pagination\n",
            plural
        ));
        code.push_str("   */\n");
        code.push_str(&format!(
            "  async list(params?: QueryParams): Promise<ListResponse<{}>> {{\n",
            model.name
        ));
        code.push_str("    const queryString = params ? '?' + new URLSearchParams(params as any).toString() : '';\n");
        code.push_str(&format!("    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}${{{{queryString}}}}`);\n", plural));
        code.push_str("    if (!response.ok) {\n");
        code.push_str("      throw new Error(`Failed to list {}: ${response.statusText}`);\n");
        code.push_str("    }\n");
        code.push_str("    return response.json();\n");
        code.push_str("  }\n\n");

        // Get method
        code.push_str("  /**\n");
        code.push_str(&format!("   * Get a single {} by ID\n", model_lower));
        code.push_str("   */\n");
        code.push_str(&format!(
            "  async get(id: string): Promise<{}> {{\n",
            model.name
        ));
        code.push_str(&format!(
            "    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}/${{{{id}}}}`);\n",
            plural
        ));
        code.push_str("    if (!response.ok) {\n");
        code.push_str("      throw new Error(`Failed to get {}: ${response.statusText}`);\n");
        code.push_str("    }\n");
        code.push_str("    return response.json();\n");
        code.push_str("  }\n\n");

        // Create method
        code.push_str("  /**\n");
        code.push_str(&format!("   * Create a new {}\n", model_lower));
        code.push_str("   */\n");
        code.push_str(&format!(
            "  async create(data: Create{}Request): Promise<{}> {{\n",
            model.name, model.name
        ));
        code.push_str(&format!(
            "    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}`, {{\n",
            plural
        ));
        code.push_str("      method: 'POST',\n");
        code.push_str("      headers: { 'Content-Type': 'application/json' },\n");
        code.push_str("      body: JSON.stringify(data),\n");
        code.push_str("    });\n");
        code.push_str("    if (!response.ok) {\n");
        code.push_str("      throw new Error(`Failed to create {}: ${response.statusText}`);\n");
        code.push_str("    }\n");
        code.push_str("    return response.json();\n");
        code.push_str("  }\n\n");

        // Update method
        code.push_str("  /**\n");
        code.push_str(&format!("   * Update an existing {}\n", model_lower));
        code.push_str("   */\n");
        code.push_str(&format!(
            "  async update(id: string, data: Update{}Request): Promise<{}> {{\n",
            model.name, model.name
        ));
        code.push_str(&format!(
            "    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}/${{{{id}}}}`, {{\n",
            plural
        ));
        code.push_str("      method: 'PUT',\n");
        code.push_str("      headers: { 'Content-Type': 'application/json' },\n");
        code.push_str("      body: JSON.stringify(data),\n");
        code.push_str("    });\n");
        code.push_str("    if (!response.ok) {\n");
        code.push_str("      throw new Error(`Failed to update {}: ${response.statusText}`);\n");
        code.push_str("    }\n");
        code.push_str("    return response.json();\n");
        code.push_str("  }\n\n");

        // Delete method
        code.push_str("  /**\n");
        code.push_str(&format!("   * Delete a {}\n", model_lower));
        code.push_str("   */\n");
        code.push_str("  async delete(id: string): Promise<void> {\n");
        code.push_str(&format!(
            "    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}/${{{{id}}}}`, {{\n",
            plural
        ));
        code.push_str("      method: 'DELETE',\n");
        code.push_str("    });\n");
        code.push_str("    if (!response.ok) {\n");
        code.push_str("      throw new Error(`Failed to delete {}: ${response.statusText}`);\n");
        code.push_str("    }\n");
        code.push_str("  }\n");

        // Generate relation methods
        for field in &model.fields {
            if let FieldType::Relation(rel_type) = &field.field_type {
                match rel_type {
                    RelationType::OneToMany(target) => {
                        let target_lower = target.to_lowercase();
                        let target_plural = format!("{}s", target_lower);
                        code.push_str("\n");
                        code.push_str("  /**\n");
                        code.push_str(&format!(
                            "   * Get related {} for this {}\n",
                            target_plural, model_lower
                        ));
                        code.push_str("   */\n");
                        code.push_str(&format!("  async {}(id: string, params?: QueryParams): Promise<ListResponse<{}>> {{\n",
                            field.name, target));
                        code.push_str("    const queryString = params ? '?' + new URLSearchParams(params as any).toString() : '';\n");
                        code.push_str(&format!("    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}/${{{{id}}}}/{}${{{{queryString}}}}`);\n",
                            plural, field.name));
                        code.push_str("    if (!response.ok) {\n");
                        code.push_str(&format!("      throw new Error(`Failed to get {} for {}: ${{response.statusText}}`);\n",
                            target_plural, model_lower));
                        code.push_str("    }\n");
                        code.push_str("    return response.json();\n");
                        code.push_str("  }\n");
                    }
                    RelationType::RequiredReference(target)
                    | RelationType::OptionalReference(target) => {
                        code.push_str("\n");
                        code.push_str("  /**\n");
                        code.push_str(&format!(
                            "   * Get the related {} for this {}\n",
                            target, model_lower
                        ));
                        code.push_str("   */\n");
                        code.push_str(&format!(
                            "  async {}(id: string): Promise<{}> {{\n",
                            field.name, target
                        ));
                        code.push_str(&format!(
                            "    const {} = await this.get(id);\n",
                            model_lower
                        ));
                        code.push_str(&format!(
                            "    const {}_id = {}.{};\n",
                            target.to_lowercase(),
                            model_lower,
                            field.name
                        ));
                        code.push_str(&format!("    if (!{}_id) {{\n", target.to_lowercase()));
                        code.push_str(&format!(
                            "      throw new Error('No {} reference found');\n",
                            target
                        ));
                        code.push_str("    }\n");
                        code.push_str(&format!("    const response = await fetch(`${{{{this.baseUrl}}}}/api/{}s/${{{{{}_id}}}}`);\n",
                            target.to_lowercase(), target.to_lowercase()));
                        code.push_str("    if (!response.ok) {\n");
                        code.push_str(&format!("      throw new Error(`Failed to get {}: ${{response.statusText}}`);\n", target));
                        code.push_str("    }\n");
                        code.push_str("    return response.json();\n");
                        code.push_str("  }\n");
                    }
                    _ => {}
                }
            }
        }

        code.push_str("}\n");

        GeneratedFile {
            path: format!("generated/sdk/{}Api.ts", model.name),
            content: code,
        }
    }

    /// Generate main index.ts file
    fn generate_index(schema: &Schema) -> GeneratedFile {
        let mut code = String::new();

        code.push_str("// Auto-generated SDK entry point\n\n");

        // Export all types
        code.push_str("export * from './types';\n\n");

        // Export all API clients
        for model in &schema.models {
            code.push_str(&format!(
                "export {{ {}Api }} from './{}Api';\n",
                model.name, model.name
            ));
        }

        code.push_str("\n// Main SDK class\n");
        code.push_str("export class SinkDBClient {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "  public {}: {}Api;\n",
                model.name.to_lowercase(),
                model.name
            ));
        }
        code.push_str("\n");
        code.push_str("  constructor(baseUrl: string) {\n");
        for model in &schema.models {
            code.push_str(&format!(
                "    this.{} = new {}Api(baseUrl);\n",
                model.name.to_lowercase(),
                model.name
            ));
        }
        code.push_str("  }\n");
        code.push_str("}\n");

        GeneratedFile {
            path: "generated/sdk/index.ts".to_string(),
            content: code,
        }
    }

    /// Generate package.json
    fn generate_package_json() -> GeneratedFile {
        let content = r#"{
  "name": "@sinkdb/client",
  "version": "0.1.0",
  "description": "Auto-generated TypeScript SDK for SinkDB API",
  "main": "./dist/index.js",
  "module": "./dist/index.mjs",
  "types": "./dist/index.d.ts",
  "exports": {
    ".": {
      "import": "./dist/index.mjs",
      "require": "./dist/index.js",
      "types": "./dist/index.d.ts"
    }
  },
  "scripts": {
    "build": "tsup",
    "dev": "tsup --watch",
    "prepublishOnly": "npm run build"
  },
  "keywords": ["sinkdb", "api", "client", "sdk", "typescript"],
  "author": "SinkDB",
  "license": "MIT",
  "devDependencies": {
    "tsup": "^8.0.0",
    "typescript": "^5.3.0"
  }
}
"#;

        GeneratedFile {
            path: "generated/sdk/package.json".to_string(),
            content: content.to_string(),
        }
    }

    /// Generate tsconfig.json
    fn generate_tsconfig() -> GeneratedFile {
        let content = r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020", "DOM"],
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "node",
    "resolveJsonModule": true
  },
  "include": ["./**/*.ts"],
  "exclude": ["node_modules", "dist"]
}
"#;

        GeneratedFile {
            path: "generated/sdk/tsconfig.json".to_string(),
            content: content.to_string(),
        }
    }

    /// Generate tsup.config.ts (bundler config)
    fn generate_tsup_config() -> GeneratedFile {
        let content = r#"import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['index.ts'],
  format: ['cjs', 'esm'],
  dts: true,
  clean: true,
  sourcemap: true,
  splitting: false,
});
"#;

        GeneratedFile {
            path: "generated/sdk/tsup.config.ts".to_string(),
            content: content.to_string(),
        }
    }

    /// Generate README.md
    fn generate_readme(schema: &Schema) -> GeneratedFile {
        let mut content = String::new();

        content.push_str("# SinkDB TypeScript SDK\n\n");
        content.push_str("Auto-generated TypeScript client for SinkDB API.\n\n");

        content.push_str("## Installation\n\n");
        content.push_str("```bash\n");
        content.push_str("npm install @sinkdb/client\n");
        content.push_str("```\n\n");

        content.push_str("## Usage\n\n");
        content.push_str("```typescript\n");
        content.push_str("import { SinkDBClient } from '@sinkdb/client';\n\n");
        content.push_str("const client = new SinkDBClient('http://localhost:3000');\n\n");

        // Show example for first model if available
        if let Some(model) = schema.models.first() {
            let model_lower = model.name.to_lowercase();
            content.push_str(&format!("// List all {}\n", model_lower));
            content.push_str(&format!(
                "const {{s}} = await client.{}.list();\n\n",
                model_lower
            ));

            content.push_str(&format!("// Get {} by ID\n", model_lower));
            content.push_str(&format!(
                "const {} = await client.{}.get('some-uuid');\n\n",
                model_lower, model_lower
            ));

            content.push_str(&format!("// Create new {}\n", model_lower));
            content.push_str(&format!(
                "const new{} = await client.{}.create({{\n",
                model.name, model_lower
            ));

            // Show first non-auto field as example
            for field in &model.fields {
                if !field.auto_generate && !Self::is_virtual_field(field) {
                    let example_value = Self::example_value_for_type(&field.field_type);
                    content.push_str(&format!("  {}: {},\n", field.name, example_value));
                    break;
                }
            }
            content.push_str("});\n\n");

            content.push_str(&format!("// Update {}\n", model_lower));
            content.push_str(&format!(
                "await client.{}.update('some-uuid', {{\n",
                model_lower
            ));
            for field in &model.fields {
                if !field.auto_generate && !Self::is_virtual_field(field) {
                    let example_value = Self::example_value_for_type(&field.field_type);
                    content.push_str(&format!("  {}: {},\n", field.name, example_value));
                    break;
                }
            }
            content.push_str("});\n\n");

            content.push_str(&format!("// Delete {}\n", model_lower));
            content.push_str(&format!(
                "await client.{}.delete('some-uuid');\n",
                model_lower
            ));
        }

        content.push_str("```\n\n");

        content.push_str("## API Reference\n\n");
        for model in &schema.models {
            content.push_str(&format!("### {}Api\n\n", model.name));
            content.push_str(&format!(
                "- `list(params?: QueryParams): Promise<ListResponse<{}>>`\n",
                model.name
            ));
            content.push_str(&format!("- `get(id: string): Promise<{}>`\n", model.name));
            content.push_str(&format!(
                "- `create(data: Create{}Request): Promise<{}>`\n",
                model.name, model.name
            ));
            content.push_str(&format!(
                "- `update(id: string, data: Update{}Request): Promise<{}>`\n",
                model.name, model.name
            ));
            content.push_str("- `delete(id: string): Promise<void>`\n");
            content.push_str("\n");
        }

        content.push_str("## Development\n\n");
        content.push_str("```bash\n");
        content.push_str("# Build the SDK\n");
        content.push_str("npm run build\n\n");
        content.push_str("# Watch mode\n");
        content.push_str("npm run dev\n");
        content.push_str("```\n");

        GeneratedFile {
            path: "generated/sdk/README.md".to_string(),
            content,
        }
    }

    /// Map FieldType to TypeScript type
    fn map_field_type_to_ts(field_type: &FieldType) -> String {
        match field_type {
            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 | FieldType::F64 => {
                "number".to_string()
            }
            FieldType::Bool => "boolean".to_string(),
            FieldType::String | FieldType::Uuid | FieldType::Timestamp => "string".to_string(),
            FieldType::Char(_) => "string".to_string(), // Char arrays as strings in TS
            FieldType::FixedArray(inner, _) => {
                format!("{}[]", Self::map_field_type_to_ts(inner))
            }
            FieldType::StructType(name) => name.clone(),
            FieldType::OptionalStructType(name) => format!("{} | null", name),
            FieldType::Relation(rel_type) => match rel_type {
                RelationType::RequiredReference(_) => "string".to_string(), // UUID FK
                RelationType::OptionalReference(_) => "string | null".to_string(),
                _ => "any".to_string(), // Virtual fields
            },
        }
    }

    /// Check if field is virtual (doesn't store data)
    fn is_virtual_field(field: &Field) -> bool {
        matches!(
            &field.field_type,
            FieldType::Relation(RelationType::OneToMany(_))
                | FieldType::Relation(RelationType::ManyToMany(_))
        )
    }

    /// Check if type is optional
    fn is_optional(field_type: &FieldType) -> bool {
        matches!(
            field_type,
            FieldType::OptionalStructType(_)
                | FieldType::Relation(RelationType::OptionalReference(_))
        )
    }

    /// Generate example value for documentation
    fn example_value_for_type(field_type: &FieldType) -> String {
        match field_type {
            FieldType::String => "'example'".to_string(),
            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 | FieldType::F64 => {
                "42".to_string()
            }
            FieldType::Bool => "true".to_string(),
            FieldType::Uuid => "'550e8400-e29b-41d4-a716-446655440000'".to_string(),
            FieldType::Timestamp => "Date.now()".to_string(),
            _ => "'...'".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{IndexType, Model};

    fn create_test_schema() -> Schema {
        Schema {
            structs: vec![],
            models: vec![Model {
                name: "User".to_string(),
                fields: vec![
                    Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        unique: false,
                        indexed: false,
                        auto_generate: true,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "email".to_string(),
                        field_type: FieldType::String,
                        unique: true,
                        indexed: true,
                        auto_generate: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                soft_delete: false,
            }],
        }
    }

    #[test]
    fn test_generate_types() {
        let schema = create_test_schema();
        let file = TypeScriptGenerator::generate_types(&schema);

        assert_eq!(file.path, "generated/sdk/types.ts");
        assert!(file.content.contains("export interface User"));
        assert!(file.content.contains("export interface CreateUserRequest"));
        assert!(file.content.contains("export interface UpdateUserRequest"));
        assert!(file.content.contains("id: string"));
        assert!(file.content.contains("email: string"));
    }

    #[test]
    fn test_generate_api_client() {
        let schema = create_test_schema();
        let model = &schema.models[0];
        let file = TypeScriptGenerator::generate_api_client(model, &schema);

        assert_eq!(file.path, "generated/sdk/UserApi.ts");
        assert!(file.content.contains("export class UserApi"));
        assert!(file.content.contains("async list("));
        assert!(file.content.contains("async get("));
        assert!(file.content.contains("async create("));
        assert!(file.content.contains("async update("));
        assert!(file.content.contains("async delete("));
    }

    #[test]
    fn test_map_field_type_to_ts() {
        assert_eq!(
            TypeScriptGenerator::map_field_type_to_ts(&FieldType::String),
            "string"
        );
        assert_eq!(
            TypeScriptGenerator::map_field_type_to_ts(&FieldType::U32),
            "number"
        );
        assert_eq!(
            TypeScriptGenerator::map_field_type_to_ts(&FieldType::Bool),
            "boolean"
        );
        assert_eq!(
            TypeScriptGenerator::map_field_type_to_ts(&FieldType::Uuid),
            "string"
        );
        assert_eq!(
            TypeScriptGenerator::map_field_type_to_ts(&FieldType::OptionalStructType(
                "Address".to_string()
            )),
            "Address | null"
        );
    }
}
