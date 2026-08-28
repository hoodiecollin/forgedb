use crate::{GeneratedCode, Result, RustGenerator};
use forgedb_parser::{FieldType, Model, RelationType, Schema, Struct};
use serde_json::{json, Map, Value};

pub struct OpenApiGenerator;

impl OpenApiGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let spec = Self::build_spec(schema);
        let code = serde_json::to_string_pretty(&spec).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!("Failed to serialize OpenAPI spec: {}", e))
        })?;

        Ok(GeneratedCode {
            code,
            description: format!("OpenAPI 3.1 specification ({} models)", schema.models.len()),
        })
    }

    fn build_spec(schema: &Schema) -> Value {
        let mut paths = Map::new();
        for model in &schema.models {
            let kebab = Self::to_kebab_case(&model.name);
            paths.insert(format!("/api/{}", kebab), Self::collection_path(model));
            paths.insert(format!("/api/{}/{{id}}", kebab), Self::item_path(model));
        }

        let mut schemas = Map::new();
        for st in &schema.structs {
            schemas.insert(st.name.clone(), Self::struct_schema(schema, st));
        }
        for en in &schema.enums {
            schemas.insert(en.name.clone(), Self::enum_schema(en));
        }
        for model in &schema.models {
            schemas.insert(model.name.clone(), Self::model_schema(schema, model));
        }

        json!({
            "openapi": "3.1.0",
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

    fn id_response() -> Value {
        json!({
            "type": "object",
            "properties": { "id": { "type": "string" } }
        })
    }

    fn model_ref(name: &str) -> Value {
        json!({ "$ref": format!("#/components/schemas/{}", name) })
    }

    fn model_schema(schema: &Schema, model: &Model) -> Value {
        Self::object_schema(schema, &model.name, &model.fields)
    }

    fn struct_schema(schema: &Schema, st: &Struct) -> Value {
        Self::object_schema(schema, &st.name, &st.fields)
    }

    fn enum_schema(en: &forgedb_parser::EnumDef) -> Value {
        let variants: Vec<Value> = en
            .variants
            .iter()
            .map(|v| Value::String(v.clone()))
            .collect();
        json!({
            "type": "string",
            "enum": variants,
            "description": format!("{} enum", en.name)
        })
    }

    fn object_schema(schema: &Schema, name: &str, fields: &[forgedb_parser::Field]) -> Value {
        let mut properties = Map::new();
        let mut required: Vec<Value> = Vec::new();

        for field in fields {
            let Some(prop) = Self::field_schema(schema, &field.field_type) else {
                continue;
            };
            properties.insert(field.name.clone(), prop);
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

    fn field_schema(schema: &Schema, field_type: &FieldType) -> Option<Value> {
        Some(match field_type {
            FieldType::U32 => json!({ "type": "integer", "format": "int32", "minimum": 0 }),
            FieldType::U64 => json!({ "type": "integer", "format": "int64", "minimum": 0 }),
            FieldType::I32 => json!({ "type": "integer", "format": "int32" }),
            FieldType::I64 => json!({ "type": "integer", "format": "int64" }),
            FieldType::F64 => json!({ "type": "number", "format": "double" }),
            FieldType::Bool => json!({ "type": "boolean" }),
            FieldType::String => json!({ "type": "string" }),
            FieldType::StringN { chars, exact } => {
                let n = *chars as u64;
                if *exact {
                    json!({ "type": "string", "minLength": n, "maxLength": n })
                } else {
                    json!({ "type": "string", "maxLength": n })
                }
            }
            FieldType::Json => json!({ "description": "Arbitrary JSON value" }),
            FieldType::Decimal => json!({ "type": "string", "format": "decimal" }),
            FieldType::Uuid => json!({ "type": "string", "format": "uuid" }),
            FieldType::Timestamp(p) => json!({
                "type": "string",
                "format": "date-time",
                "description": format!(
                    "RFC 3339 instant, declared precision `{}` (stored as microseconds)",
                    p.key()
                )
            }),
            FieldType::Bytes(n) => json!({
                "type": "array",
                "items": { "type": "integer", "minimum": 0, "maximum": 255 },
                "minItems": *n,
                "maxItems": *n,
                "description": format!("Fixed {}-byte array", n)
            }),
            FieldType::FixedArray(inner, n) => {
                let items = Self::field_schema(schema, inner)?;
                json!({
                    "type": "array",
                    "items": items,
                    "minItems": *n,
                    "maxItems": *n
                })
            }
            FieldType::Enum(enum_name) => Self::model_ref(enum_name),
            FieldType::StructType(struct_name) => Self::model_ref(struct_name),
            FieldType::OptionalStructType(struct_name) => {
                json!({
                    "anyOf": [ Self::model_ref(struct_name), { "type": "null" } ]
                })
            }
            FieldType::Nullable(inner) => {
                let mut inner_schema = Self::field_schema(schema, inner)?;
                Self::make_nullable(&mut inner_schema);
                inner_schema
            }
            FieldType::Relation(rel) => match rel {
                RelationType::RequiredReference(target)
                | RelationType::OptionalReference(target) => {
                    let optional = matches!(rel, RelationType::OptionalReference(_));
                    let key = RustGenerator::fk_backing_type(schema, field_type)
                        .unwrap_or(FieldType::Uuid);
                    let key = match key {
                        FieldType::Nullable(inner) => *inner,
                        k => k,
                    };
                    let mut fk = Self::field_schema(schema, &key)?;
                    if optional {
                        Self::make_nullable(&mut fk);
                    }
                    if let Some(obj) = fk.as_object_mut() {
                        obj.insert(
                            "description".to_string(),
                            Value::String(format!("Foreign key → {}", target)),
                        );
                    }
                    fk
                }
                RelationType::OneToMany(_) | RelationType::ManyToMany(_) => return None,
            },
            FieldType::Component(_) => return None,
        })
    }

    fn make_nullable(schema: &mut Value) {
        match schema.get("type") {
            Some(Value::String(t)) => {
                let t = t.clone();
                if let Some(obj) = schema.as_object_mut() {
                    obj.insert(
                        "type".to_string(),
                        Value::Array(vec![Value::String(t), Value::String("null".to_string())]),
                    );
                }
            }
            Some(Value::Array(types)) => {
                let mut types = types.clone();
                if !types.iter().any(|v| v == "null") {
                    types.push(Value::String("null".to_string()));
                }
                if let Some(obj) = schema.as_object_mut() {
                    obj.insert("type".to_string(), Value::Array(types));
                }
            }
            _ => {
                let inner = schema.clone();
                *schema = json!({ "anyOf": [inner, { "type": "null" }] });
            }
        }
    }

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
