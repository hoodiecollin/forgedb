//! Markdown documentation generation
//!
//! Generates API documentation in markdown format.

use crate::ast::{Field, FieldType, Model, RelationType, Schema};
use crate::codegen::{naming, semantics, GeneratedFile};

/// Generate markdown documentation
pub fn generate_markdown_docs(schema: &Schema) -> GeneratedFile {
    let mut content = String::new();

    content.push_str("# API Documentation\n\n");
    content.push_str("Auto-generated from ForgeDB schema.\n\n");

    content.push_str("## Table of Contents\n\n");
    for model in &schema.models {
        content.push_str(&format!(
            "- [{}](#{})\n",
            model.name,
            model.name.to_lowercase()
        ));
    }
    content.push_str("\n---\n\n");

    // Document each model
    for model in &schema.models {
        document_model(&mut content, model);
    }

    GeneratedFile {
        path: "generated/openapi/API.md".to_string(),
        content,
    }
}

/// Document a single model in markdown
fn document_model(content: &mut String, model: &Model) {
    let model_lower = model.name.to_lowercase();
    let model_plural = naming::pluralize(&model_lower);

    content.push_str(&format!("## {}\n\n", model.name));

    // Fields table
    content.push_str("### Fields\n\n");
    content.push_str("| Field | Type | Constraints | Description |\n");
    content.push_str("|-------|------|-------------|-------------|\n");

    for field in &model.fields {
        if semantics::is_virtual_field(field) {
            continue;
        }

        let field_name = semantics::relation_field_name(field);
        let field_type = type_to_markdown(&field.field_type);

        let mut constraints = Vec::new();
        if field.auto_generate {
            constraints.push("auto-generated");
        }
        if field.unique {
            constraints.push("unique");
        }
        if field.indexed {
            constraints.push("indexed");
        }
        if field.is_computed {
            constraints.push("computed");
        }
        for constraint in &field.constraints {
            constraints.push(&constraint.name);
        }
        let constraints_str = constraints.join(", ");

        let description = if field.is_computed {
            "Computed field (read-only)"
        } else {
            ""
        };

        content.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            field_name, field_type, constraints_str, description
        ));
    }
    content.push_str("\n");

    // API Endpoints
    content.push_str("### Endpoints\n\n");

    content.push_str(&format!("#### List {}\n", model_plural));
    content.push_str(&format!("```\nGET /api/{}\n```\n\n", model_plural));
    content.push_str("Query Parameters:\n");
    content.push_str("- `limit` (integer): Maximum items to return\n");
    content.push_str("- `offset` (integer): Number of items to skip\n");
    content.push_str("- `sort` (string): Field to sort by\n\n");

    content.push_str(&format!("#### Create {}\n", model_lower));
    content.push_str(&format!("```\nPOST /api/{}\n```\n\n", model_plural));

    content.push_str(&format!("#### Get {} by ID\n", model_lower));
    content.push_str(&format!("```\nGET /api/{}/{{id}}\n```\n\n", model_plural));

    content.push_str(&format!("#### Update {}\n", model_lower));
    content.push_str(&format!("```\nPUT /api/{}/{{id}}\n```\n\n", model_plural));

    content.push_str(&format!("#### Delete {}\n", model_lower));
    content.push_str(&format!(
        "```\nDELETE /api/{}/{{id}}\n```\n\n",
        model_plural
    ));

    content.push_str("---\n\n");
}

/// Convert FieldType to markdown-friendly type string
fn type_to_markdown(field_type: &FieldType) -> String {
    match field_type {
        FieldType::String => "string".to_string(),
        FieldType::U32 => "u32".to_string(),
        FieldType::U64 => "u64".to_string(),
        FieldType::I32 => "i32".to_string(),
        FieldType::I64 => "i64".to_string(),
        FieldType::F64 => "f64".to_string(),
        FieldType::Bool => "bool".to_string(),
        FieldType::Uuid => "uuid".to_string(),
        FieldType::Timestamp => "timestamp".to_string(),
        FieldType::Char(n) => format!("char[{}]", n),
        FieldType::FixedArray(inner, n) => {
            format!("[{}; {}]", type_to_markdown(inner), n)
        }
        FieldType::StructType(name) => name.clone(),
        FieldType::OptionalStructType(name) => format!("{}?", name),
        FieldType::Relation(rel) => match rel {
            RelationType::RequiredReference(target) => format!("*{}", target),
            RelationType::OptionalReference(target) => format!("?{}", target),
            RelationType::OneToMany(target) => format!("[{}]", target),
            RelationType::ManyToMany(target) => format!("[{}]", target),
        },
        FieldType::Component(comp_ref) => {
            use crate::ast::ComponentProtocol;
            let protocol = match comp_ref.protocol {
                ComponentProtocol::Tsx => "tsx",
                ComponentProtocol::Jsx => "jsx",
                ComponentProtocol::Api => "api",
            };
            format!("{}://{}", protocol, comp_ref.path)
        }
    }
}
