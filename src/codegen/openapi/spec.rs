//! OpenAPI specification generation
//!
//! Generates OpenAPI 3.0 specification from schema definitions.
//! Uses IR for consistent field classification and constraints module for validation mapping.

use crate::ast::{Field, FieldType, RelationType, Schema};
use crate::codegen::{constraints, ir::IrSchema, naming, CodegenConfig, GeneratedFile};
use serde_json::{json, Value};

/// Generate the main OpenAPI 3.0 specification
pub fn generate_openapi_spec(schema: &Schema) -> GeneratedFile {
    generate_openapi_spec_with_config(schema, &CodegenConfig::default())
}

/// Generate the main OpenAPI 3.0 specification with custom config
pub fn generate_openapi_spec_with_config(schema: &Schema, config: &CodegenConfig) -> GeneratedFile {
    let ir_schema = IrSchema::from_ast(schema.clone());
    let mut spec = json!({
        "openapi": "3.0.3",
        "info": {
            "title": "ForgeDB Generated API",
            "description": "Auto-generated REST API from ForgeDB schema",
            "version": "1.0.0"
        },
        "servers": [
            {
                "url": "http://localhost:3000",
                "description": "Development server"
            }
        ],
        "paths": {},
        "components": {
            "schemas": {}
        }
    });

    // Generate schemas for all models using IR
    for ir_model in &ir_schema.models {
        add_model_schemas_from_ir(&mut spec, ir_model);
    }

    // Generate paths for all models using IR
    for ir_model in &ir_schema.models {
        add_model_paths_from_ir(&mut spec, ir_model, config);
    }

    GeneratedFile {
        path: format!("{}/openapi.json", config.paths.openapi),
        content: serde_json::to_string_pretty(&spec).unwrap(),
    }
}

/// Add schemas for a model using IR (Model, CreateRequest, UpdateRequest)
fn add_model_schemas_from_ir(spec: &mut Value, ir_model: &crate::codegen::ir::IrModel) {
    // Main model schema - includes stored fields and computed fields
    let model_schema = {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        // Add stored fields
        for ir_field in &ir_model.stored_fields {
            let (field_name, field_schema) = field_to_openapi_schema(&ir_field.original);
            properties.insert(field_name.clone(), field_schema);

            // Required if not optional and not auto-generated
            if !ir_field.is_optional && !ir_field.is_auto_generate {
                required.push(field_name);
            }
        }

        // Add computed fields to model schema
        for ir_field in &ir_model.computed_fields {
            let (field_name, field_schema) = field_to_openapi_schema(&ir_field.original);
            properties.insert(field_name, field_schema);
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    };

    spec["components"]["schemas"]
        .as_object_mut()
        .unwrap()
        .insert(ir_model.name.clone(), model_schema);

    // CreateRequest schema - uses IR's create_request_fields
    let create_schema = {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for ir_field in ir_model.create_request_fields() {
            let (field_name, field_schema) = field_to_openapi_schema(&ir_field.original);
            properties.insert(field_name.clone(), field_schema);

            if !ir_field.is_optional {
                required.push(field_name);
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    };

    spec["components"]["schemas"]
        .as_object_mut()
        .unwrap()
        .insert(format!("Create{}Request", ir_model.name), create_schema);

    // UpdateRequest schema - uses IR's update_request_fields (all optional)
    let mut update_schema = json!({
        "type": "object",
        "properties": {}
    });

    let update_props = update_schema["properties"].as_object_mut().unwrap();

    for ir_field in ir_model.update_request_fields() {
        let (field_name, field_schema) = field_to_openapi_schema(&ir_field.original);
        update_props.insert(field_name, field_schema);
    }

    spec["components"]["schemas"]
        .as_object_mut()
        .unwrap()
        .insert(format!("Update{}Request", ir_model.name), update_schema);
}

/// Add API paths for a model using IR
fn add_model_paths_from_ir(spec: &mut Value, ir_model: &crate::codegen::ir::IrModel, config: &CodegenConfig) {
    let paths = spec["paths"].as_object_mut().unwrap();
    let model_lower = ir_model.name.to_lowercase();
    let model_plural = ir_model.relation_name_for_api();

    // List endpoint: GET /api/models
    paths.insert(
        format!("{}/{}", config.api_base, model_plural),
        json!({
            "get": {
                "summary": format!("List all {}", model_plural),
                "tags": [ir_model.name],
                "parameters": [
                    {
                        "name": "limit",
                        "in": "query",
                        "schema": { "type": "integer", "minimum": 1, "maximum": 1000 },
                        "description": "Maximum number of items to return"
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "schema": { "type": "integer", "minimum": 0 },
                        "description": "Number of items to skip"
                    },
                    {
                        "name": "sort",
                        "in": "query",
                        "schema": { "type": "string" },
                        "description": "Field to sort by (prefix with - for descending)"
                    },
                    {
                        "name": "fields",
                        "in": "query",
                        "schema": { "type": "string" },
                        "description": "Comma-separated list of fields to include in response (e.g., 'id,name,email')"
                    },
                    {
                        "name": "include_deleted",
                        "in": "query",
                        "schema": { "type": "boolean" },
                        "description": "Include soft-deleted records (only for models with @soft_delete)"
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Successful response",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "data": {
                                            "type": "array",
                                            "items": { "$ref": format!("#/components/schemas/{}", ir_model.name) }
                                        },
                                        "total": {
                                            "type": "integer",
                                            "description": "Total number of items matching the query"
                                        },
                                        "limit": {
                                            "type": "integer",
                                            "description": "Maximum number of items returned"
                                        },
                                        "offset": {
                                            "type": "integer",
                                            "description": "Number of items skipped"
                                        }
                                    },
                                    "required": ["data", "total"]
                                }
                            }
                        }
                    }
                }
            },
            "post": {
                "summary": format!("Create a new {}", model_lower),
                "tags": [ir_model.name],
                "requestBody": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/Create{}Request", ir_model.name) }
                        }
                    }
                },
                "responses": {
                    "201": {
                        "description": "Created",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": format!("#/components/schemas/{}", ir_model.name) }
                            }
                        }
                    },
                    "400": {
                        "description": "Validation error",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "error": { "type": "string" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }),
    );

    // Single item endpoints: GET/PUT/DELETE /api/models/:id
    if let Some(_id_field) = &ir_model.id_field {
        paths.insert(
            format!("{}/{}/{{id}}", config.api_base, model_plural),
            json!({
                "get": {
                    "summary": format!("Get a {} by ID", model_lower),
                    "tags": [ir_model.name],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" },
                            "description": format!("{} ID", ir_model.name)
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Successful response",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": format!("#/components/schemas/{}", ir_model.name) }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found"
                        }
                    }
                },
                "put": {
                    "summary": format!("Update a {}", model_lower),
                    "tags": [ir_model.name],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" },
                            "description": format!("{} ID", ir_model.name)
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": format!("#/components/schemas/Update{}Request", ir_model.name) }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": format!("#/components/schemas/{}", ir_model.name) }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found"
                        },
                        "400": {
                            "description": "Validation error"
                        }
                    }
                },
                "delete": {
                    "summary": format!("Delete a {}", model_lower),
                    "tags": [ir_model.name],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" },
                            "description": format!("{} ID", ir_model.name)
                        }
                    ],
                    "responses": {
                        "204": {
                            "description": "Deleted successfully"
                        },
                        "404": {
                            "description": "Not found"
                        }
                    }
                }
            }),
        );

        // Batch operations endpoints
        paths.insert(
            format!("{}/{}/batch", config.api_base, model_plural),
            json!({
                "post": {
                    "summary": format!("Batch create {} records", model_plural),
                    "tags": [ir_model.name],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": format!("#/components/schemas/Create{}Request", ir_model.name) }
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Batch created successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": format!("#/components/schemas/{}", ir_model.name) }
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Validation error"
                        }
                    }
                },
                "delete": {
                    "summary": format!("Batch delete {} records", model_plural),
                    "tags": [ir_model.name],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "ids": {
                                            "type": "array",
                                            "items": { "type": "string", "format": "uuid" }
                                        }
                                    },
                                    "required": ["ids"]
                                }
                            }
                        }
                    },
                    "responses": {
                        "204": {
                            "description": "Batch deleted successfully"
                        },
                        "400": {
                            "description": "Validation error"
                        }
                    }
                }
            }),
        );
    }
}

/// Convert a field to OpenAPI schema using centralized constraints mapping
fn field_to_openapi_schema(field: &Field) -> (String, Value) {
    use crate::codegen::semantics;
    
    let field_name = semantics::relation_field_name(field);

    let mut schema = type_to_openapi_type(&field.field_type);

    // Add description for computed fields
    if field.is_computed {
        schema.as_object_mut().unwrap().insert(
            "description".to_string(),
            json!("Computed field (read-only)"),
        );
    }

    // Add validation constraints using centralized mapping
    let constraint_props = constraints::get_openapi_properties(&field.constraints, &field.field_type);
    if let Some(schema_obj) = schema.as_object_mut() {
        for (key, value) in constraint_props {
            schema_obj.insert(key, value);
        }
    }

    (field_name, schema)
}

/// Convert FieldType to OpenAPI type specification
fn type_to_openapi_type(field_type: &FieldType) -> Value {
    match field_type {
        FieldType::String => json!({ "type": "string" }),
        FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => {
            json!({ "type": "integer" })
        }
        FieldType::F64 => json!({ "type": "number", "format": "double" }),
        FieldType::Bool => json!({ "type": "boolean" }),
        FieldType::Uuid => json!({ "type": "string", "format": "uuid" }),
        FieldType::Timestamp => json!({ "type": "string", "format": "date-time" }),
        FieldType::Char(_) => json!({ "type": "string" }),
        FieldType::FixedArray(inner, _) => {
            json!({
                "type": "array",
                "items": type_to_openapi_type(inner)
            })
        }
        FieldType::StructType(name) | FieldType::OptionalStructType(name) => {
            json!({ "$ref": format!("#/components/schemas/{}", name) })
        }
        FieldType::Relation(rel) => match rel {
            RelationType::RequiredReference(_) => json!({ "type": "string", "format": "uuid" }),
            RelationType::OptionalReference(_) => {
                json!({ "type": "string", "format": "uuid", "nullable": true })
            }
            _ => json!({ "type": "string" }), // Virtual fields
        },
        FieldType::Component(_) => json!({ "type": "string", "description": "Component reference (virtual)" }),
    }
}
