//! OpenAPI specification generation
//!
//! Generates OpenAPI 3.0 specification from schema definitions.

use crate::ast::{Field, FieldType, Model, RelationType, Schema};
use crate::codegen::{naming, semantics, GeneratedFile};
use serde_json::{json, Value};

/// Generate the main OpenAPI 3.0 specification
pub fn generate_openapi_spec(schema: &Schema) -> GeneratedFile {
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

    // Generate schemas for all models
    for model in &schema.models {
        add_model_schemas(&mut spec, model);
    }

    // Generate paths for all models
    for model in &schema.models {
        add_model_paths(&mut spec, model);
    }

    GeneratedFile {
        path: "generated/openapi/openapi.json".to_string(),
        content: serde_json::to_string_pretty(&spec).unwrap(),
    }
}

/// Add schemas for a model (Model, CreateRequest, UpdateRequest)
fn add_model_schemas(spec: &mut Value, model: &Model) {
    // Main model schema
    let model_schema = {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for field in &model.fields {
            if semantics::is_virtual_field(field) {
                continue;
            }

            let (field_name, field_schema) = field_to_openapi_schema(field);
            properties.insert(field_name.clone(), field_schema);

            // Required if not optional and not auto-generated
            if !semantics::is_optional_field(field) {
                required.push(field_name);
            }
        }

        // Add computed fields to model schema
        for field in &model.fields {
            if field.is_computed {
                let (field_name, field_schema) = field_to_openapi_schema(field);
                properties.insert(field_name, field_schema);
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
        .insert(model.name.clone(), model_schema);

    // CreateRequest schema (no auto-generated or computed fields)
    let create_schema = {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for field in &model.fields {
            if field.auto_generate || field.is_computed || semantics::is_virtual_field(field) {
                continue;
            }

            let (field_name, field_schema) = field_to_openapi_schema(field);
            properties.insert(field_name.clone(), field_schema);

            if !semantics::is_optional_field(field) {
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
        .insert(format!("Create{}Request", model.name), create_schema);

    // UpdateRequest schema (all fields optional except computed)
    let mut update_schema = json!({
        "type": "object",
        "properties": {}
    });

    let update_props = update_schema["properties"].as_object_mut().unwrap();

    for field in &model.fields {
        if field.auto_generate || field.is_computed || semantics::is_virtual_field(field) {
            continue;
        }

        let (field_name, field_schema) = field_to_openapi_schema(field);
        update_props.insert(field_name, field_schema);
    }

    spec["components"]["schemas"]
        .as_object_mut()
        .unwrap()
        .insert(format!("Update{}Request", model.name), update_schema);
}

/// Add API paths for a model
fn add_model_paths(spec: &mut Value, model: &Model) {
    let paths = spec["paths"].as_object_mut().unwrap();
    let model_lower = model.name.to_lowercase();
    let model_plural = naming::pluralize(&model_lower);

    // List endpoint: GET /api/models
    paths.insert(
        format!("/api/{}", model_plural),
        json!({
            "get": {
                "summary": format!("List all {}", model_plural),
                "tags": [model.name],
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
                                            "items": { "$ref": format!("#/components/schemas/{}", model.name) }
                                        },
                                        "count": { "type": "integer" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "post": {
                "summary": format!("Create a new {}", model_lower),
                "tags": [model.name],
                "requestBody": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/Create{}Request", model.name) }
                        }
                    }
                },
                "responses": {
                    "201": {
                        "description": "Created",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": format!("#/components/schemas/{}", model.name) }
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
    let id_field = model.fields.iter().find(|f| f.auto_generate);
    if let Some(id_field) = id_field {
        paths.insert(
            format!("/api/{}/{{id}}", model_plural),
            json!({
                "get": {
                    "summary": format!("Get a {} by ID", model_lower),
                    "tags": [model.name],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": type_to_openapi_type(&id_field.field_type),
                            "description": format!("{} ID", model.name)
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Successful response",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": format!("#/components/schemas/{}", model.name) }
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
                    "tags": [model.name],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": type_to_openapi_type(&id_field.field_type),
                            "description": format!("{} ID", model.name)
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": format!("#/components/schemas/Update{}Request", model.name) }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": format!("#/components/schemas/{}", model.name) }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found"
                        }
                    }
                },
                "delete": {
                    "summary": format!("Delete a {}", model_lower),
                    "tags": [model.name],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": type_to_openapi_type(&id_field.field_type),
                            "description": format!("{} ID", model.name)
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
            format!("/api/{}/batch", model_plural),
            json!({
                "post": {
                    "summary": format!("Batch create {} records", model_plural),
                    "tags": [model.name],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": format!("#/components/schemas/Create{}Request", model.name) }
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
                                        "items": { "$ref": format!("#/components/schemas/{}", model.name) }
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
                    "tags": [model.name],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "ids": {
                                            "type": "array",
                                            "items": type_to_openapi_type(&id_field.field_type)
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

/// Convert a field to OpenAPI schema
fn field_to_openapi_schema(field: &Field) -> (String, Value) {
    let field_name = match &field.field_type {
        FieldType::Relation(rel) if rel.is_reference() => format!("{}_id", field.name),
        _ => field.name.clone(),
    };

    let mut schema = type_to_openapi_type(&field.field_type);

    // Add description for computed fields
    if field.is_computed {
        schema.as_object_mut().unwrap().insert(
            "description".to_string(),
            json!("Computed field (read-only)"),
        );
    }

    // Add validation constraints
    for constraint in &field.constraints {
        match constraint.name.as_str() {
            "email" => {
                schema
                    .as_object_mut()
                    .unwrap()
                    .insert("format".to_string(), json!("email"));
            }
            "url" => {
                schema
                    .as_object_mut()
                    .unwrap()
                    .insert("format".to_string(), json!("uri"));
            }
            "min" => {
                if let Some(crate::ast::ConstraintParam::Number(min_val)) =
                    constraint.params.first()
                {
                    schema
                        .as_object_mut()
                        .unwrap()
                        .insert("minimum".to_string(), json!(min_val));
                }
            }
            "max" => {
                if let Some(crate::ast::ConstraintParam::Number(max_val)) =
                    constraint.params.first()
                {
                    schema
                        .as_object_mut()
                        .unwrap()
                        .insert("maximum".to_string(), json!(max_val));
                }
            }
            "pattern" => {
                if let Some(crate::ast::ConstraintParam::String(pattern)) =
                    constraint.params.first()
                {
                    schema
                        .as_object_mut()
                        .unwrap()
                        .insert("pattern".to_string(), json!(pattern));
                }
            }
            _ => {}
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
