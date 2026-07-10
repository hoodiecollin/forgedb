//! OpenAPI 3.0 specification generator.
//!
//! Emits a standalone OpenAPI document describing the REST surface the
//! [`ApiGenerator`](crate::ApiGenerator) produces — **without compiling or
//! running the generated crate**. It mirrors the actual generated routes
//! (`/api/<kebab>` list + create, `/api/<kebab>/{id}` get + replace + delete)
//! and the serialized model shapes, so `forgedb generate openapi` yields the
//! spec at schema-compile time.
//!
//! This is deliberately distinct from the runtime `utoipa` `ApiDoc` /
//! `openapi_json()` emitted into the generated `api.rs`: that path requires
//! building and running the app to produce a spec, whereas this is the offline,
//! generate-time artifact the CLI writes to `openapi.json`.
//!
//! Identity note: this is a generator (schema → spec string), not a runtime
//! engine. It reads the compile-time schema and emits a tailored document; it
//! ships nothing that interprets a schema at runtime.

use crate::{GeneratedCode, Result};
use forgedb_parser::{FieldType, Model, RelationType, Schema, Struct};
use serde_json::{json, Map, Value};

/// OpenAPI specification generator.
pub struct OpenApiGenerator;

impl OpenApiGenerator {
    /// Generate an OpenAPI 3.0 spec (pretty-printed JSON) from a schema.
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let spec = Self::build_spec(schema);
        let code = serde_json::to_string_pretty(&spec).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!("Failed to serialize OpenAPI spec: {}", e))
        })?;

        Ok(GeneratedCode {
            code,
            description: format!("OpenAPI 3.0 specification ({} models)", schema.models.len()),
        })
    }

    /// Build the full OpenAPI document as a JSON value.
    fn build_spec(schema: &Schema) -> Value {
        let mut paths = Map::new();
        for model in &schema.models {
            let kebab = Self::to_kebab_case(&model.name);
            paths.insert(format!("/api/{}", kebab), Self::collection_path(model));
            paths.insert(format!("/api/{}/{{id}}", kebab), Self::item_path(model));
        }

        let mut schemas = Map::new();
        // Component schemas: one per model, plus one per inline struct a model
        // field may `$ref`.
        for st in &schema.structs {
            schemas.insert(st.name.clone(), Self::struct_schema(st));
        }
        for model in &schema.models {
            schemas.insert(model.name.clone(), Self::model_schema(model));
        }

        json!({
            "openapi": "3.0.3",
            "info": {
                "title": "ForgeDB Generated API",
                "version": "1.0.0",
                "description": "Auto-generated from a ForgeDB schema. Describes the \
                                REST surface emitted by the ForgeDB code generator."
            },
            "servers": [
                { "url": "http://localhost:3000", "description": "Local generated server" }
            ],
            "paths": Value::Object(paths),
            "components": { "schemas": Value::Object(schemas) }
        })
    }

    /// The `/api/<kebab>` path item: `GET` (list) + `POST` (create).
    fn collection_path(model: &Model) -> Value {
        let name = &model.name;
        let model_ref = Self::model_ref(name);
        json!({
            "get": {
                "summary": format!("List all {}", name),
                "operationId": format!("list{}", name),
                "tags": [name],
                "responses": {
                    "200": {
                        "description": format!("List of {}", name),
                        "content": { "application/json": { "schema": {
                            "type": "object",
                            "properties": { "data": { "type": "array", "items": model_ref } }
                        } } }
                    }
                }
            },
            "post": {
                "summary": format!("Create new {}", name),
                "operationId": format!("create{}", name),
                "tags": [name],
                "requestBody": {
                    "required": true,
                    "content": { "application/json": { "schema": model_ref } }
                },
                "responses": {
                    "201": {
                        "description": format!("{} created", name),
                        "content": { "application/json": { "schema": Self::id_response() } }
                    },
                    "422": { "description": "Invalid payload" }
                }
            }
        })
    }

    /// The `/api/<kebab>/{id}` path item: `GET` + `PUT` (replace) + `DELETE`.
    fn item_path(model: &Model) -> Value {
        let name = &model.name;
        let model_ref = Self::model_ref(name);
        let id_param = json!({
            "name": "id",
            "in": "path",
            "required": true,
            "description": format!("Identity of the {}", name),
            "schema": { "type": "string" }
        });
        let not_found = json!({ "description": "Not found" });
        json!({
            "parameters": [id_param],
            "get": {
                "summary": format!("Get {} by ID", name),
                "operationId": format!("get{}", name),
                "tags": [name],
                "responses": {
                    "200": {
                        "description": name,
                        "content": { "application/json": { "schema": model_ref } }
                    },
                    "400": { "description": "Invalid id" },
                    "404": not_found
                }
            },
            "put": {
                "summary": format!("Replace {} by ID", name),
                "operationId": format!("replace{}", name),
                "tags": [name],
                "requestBody": {
                    "required": true,
                    "content": { "application/json": { "schema": model_ref } }
                },
                "responses": {
                    "200": {
                        "description": format!("{} replaced", name),
                        "content": { "application/json": { "schema": Self::id_response() } }
                    },
                    "400": { "description": "Invalid id" },
                    "404": not_found,
                    "422": { "description": "Invalid payload" }
                }
            },
            "delete": {
                "summary": format!("Delete {} by ID", name),
                "operationId": format!("delete{}", name),
                "tags": [name],
                "responses": {
                    "204": { "description": format!("{} deleted", name) },
                    "400": { "description": "Invalid id" },
                    "404": not_found
                }
            }
        })
    }

    /// The `{ "id": "..." }` body returned by create/replace.
    fn id_response() -> Value {
        json!({
            "type": "object",
            "properties": { "id": { "type": "string" } }
        })
    }

    /// A `$ref` to a model's component schema.
    fn model_ref(name: &str) -> Value {
        json!({ "$ref": format!("#/components/schemas/{}", name) })
    }

    /// Component schema for a model: the stored/serialized scalar and FK fields.
    fn model_schema(model: &Model) -> Value {
        Self::object_schema(&model.name, &model.fields)
    }

    /// Component schema for an inline struct (all fields are fixed-size scalars).
    fn struct_schema(st: &Struct) -> Value {
        Self::object_schema(&st.name, &st.fields)
    }

    /// Build an `object` schema from a field list, skipping virtual fields that
    /// carry no data payload (one-to-many / many-to-many collections and
    /// component references — they serialize to `null` and are never part of a
    /// create/replace body). Non-nullable fields are marked `required`.
    fn object_schema(name: &str, fields: &[forgedb_parser::Field]) -> Value {
        let mut properties = Map::new();
        let mut required: Vec<Value> = Vec::new();

        for field in fields {
            let Some(schema) = Self::field_schema(&field.field_type) else {
                continue; // virtual / component field — not represented in the body
            };
            properties.insert(field.name.clone(), schema);
            if !field.is_nullable() {
                required.push(Value::String(field.name.clone()));
            }
        }

        let mut obj = Map::new();
        obj.insert("type".to_string(), Value::String("object".to_string()));
        obj.insert(
            "description".to_string(),
            Value::String(format!("{} model", name)),
        );
        obj.insert("properties".to_string(), Value::Object(properties));
        if !required.is_empty() {
            obj.insert("required".to_string(), Value::Array(required));
        }
        Value::Object(obj)
    }

    /// Map a field type to its OpenAPI schema, or `None` for virtual fields that
    /// have no serialized data value.
    fn field_schema(field_type: &FieldType) -> Option<Value> {
        Some(match field_type {
            FieldType::U32 => json!({ "type": "integer", "format": "int32", "minimum": 0 }),
            FieldType::U64 => json!({ "type": "integer", "format": "int64", "minimum": 0 }),
            FieldType::I32 => json!({ "type": "integer", "format": "int32" }),
            FieldType::I64 => json!({ "type": "integer", "format": "int64" }),
            FieldType::F64 => json!({ "type": "number", "format": "double" }),
            FieldType::Bool => json!({ "type": "boolean" }),
            FieldType::String => json!({ "type": "string" }),
            FieldType::Uuid => json!({ "type": "string", "format": "uuid" }),
            FieldType::Timestamp => json!({
                "type": "integer",
                "format": "int64",
                "description": "Unix timestamp"
            }),
            // char(N) serializes as an N-byte array (`[u8; N]`), so it is a
            // fixed-length array of bytes on the wire, not a string.
            FieldType::Char(n) => json!({
                "type": "array",
                "items": { "type": "integer", "minimum": 0, "maximum": 255 },
                "minItems": *n,
                "maxItems": *n,
                "description": format!("Fixed {}-byte array", n)
            }),
            FieldType::FixedArray(inner, n) => {
                let items = Self::field_schema(inner)?;
                json!({
                    "type": "array",
                    "items": items,
                    "minItems": *n,
                    "maxItems": *n
                })
            }
            FieldType::StructType(struct_name) => Self::model_ref(struct_name),
            FieldType::OptionalStructType(struct_name) => {
                // OpenAPI 3.0 can't add `nullable` directly to a `$ref`; wrap it.
                json!({
                    "allOf": [ Self::model_ref(struct_name) ],
                    "nullable": true
                })
            }
            FieldType::Nullable(inner) => {
                let mut schema = Self::field_schema(inner)?;
                Self::make_nullable(&mut schema);
                schema
            }
            FieldType::Relation(rel) => match rel {
                RelationType::RequiredReference(target) => json!({
                    "type": "string",
                    "format": "uuid",
                    "description": format!("Foreign key → {}", target)
                }),
                RelationType::OptionalReference(target) => json!({
                    "type": "string",
                    "format": "uuid",
                    "nullable": true,
                    "description": format!("Foreign key → {}", target)
                }),
                // Virtual collections have no scalar body value.
                RelationType::OneToMany(_) | RelationType::ManyToMany(_) => return None,
            },
            FieldType::Component(_) => return None,
        })
    }

    /// Mark a schema value nullable, wrapping a bare `$ref` in `allOf` (OpenAPI
    /// 3.0 forbids sibling keywords next to `$ref`).
    fn make_nullable(schema: &mut Value) {
        if schema.get("$ref").is_some() {
            let inner = schema.clone();
            *schema = json!({ "allOf": [inner], "nullable": true });
        } else if let Some(obj) = schema.as_object_mut() {
            obj.insert("nullable".to_string(), Value::Bool(true));
        }
    }

    /// Convert PascalCase to kebab-case — identical to `ApiGenerator`'s route
    /// casing, so the documented paths match the generated ones.
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
