use crate::ast::{ComponentProtocol, ComponentReference, FieldType, Model, Schema};
use crate::codegen::GeneratedFile;

pub struct RouteHandlerGenerator;

/// HTTP methods for route handlers
#[derive(Debug, Clone, Copy)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
        }
    }

    fn filename(&self) -> &'static str {
        match self {
            HttpMethod::Get => "get.ts",
            HttpMethod::Post => "post.ts",
            HttpMethod::Put => "put.ts",
            HttpMethod::Delete => "delete.ts",
            HttpMethod::Patch => "patch.ts",
        }
    }
}

impl RouteHandlerGenerator {
    pub fn new() -> Self {
        RouteHandlerGenerator
    }

    /// Generate route handler stubs for all API component fields
    pub fn generate_handlers(&self, schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = vec![];

        for model in &schema.models {
            for field in &model.fields {
                if let FieldType::Component(component_ref) = &field.field_type {
                    if matches!(component_ref.protocol, ComponentProtocol::Api) {
                        // For API routes, we'll default to POST method
                        // In the future, this could be configurable via directives
                        files.push(self.generate_route_handler(
                            &model.name,
                            &field.name,
                            component_ref,
                            HttpMethod::Post,
                        ));
                    }
                }
            }
        }

        files
    }

    /// Generate a single route handler
    fn generate_route_handler(
        &self,
        model_name: &str,
        field_name: &str,
        component_ref: &ComponentReference,
        method: HttpMethod,
    ) -> GeneratedFile {
        let mut code = String::new();

        // Import Next.js types (assuming Next.js App Router)
        code.push_str("import { NextRequest, NextResponse } from 'next/server';\n\n");

        // Generate function name based on HTTP method
        let function_name = method.as_str();

        code.push_str(&format!("export async function {}(req: NextRequest) {{\n", function_name));
        code.push_str("  try {\n");

        // Add example body parsing for POST/PUT/PATCH
        if matches!(method, HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch) {
            code.push_str("    const body = await req.json();\n\n");
            code.push_str("    // TODO: Validate request body\n");
            code.push_str("    // TODO: Process request\n\n");
        } else {
            code.push_str("    // TODO: Process request\n\n");
        }

        // Add comment about the purpose
        code.push_str(&format!(
            "    // This handler is for the '{}' action on {} model\n",
            field_name, model_name
        ));
        code.push_str("    // Implement your business logic here\n\n");

        code.push_str("    return NextResponse.json({\n");
        code.push_str("      message: 'Not implemented yet',\n");
        code.push_str("      // Add your response data here\n");
        code.push_str("    });\n");
        code.push_str("  } catch (error) {\n");
        code.push_str("    console.error('Route handler error:', error);\n");
        code.push_str("    return NextResponse.json(\n");
        code.push_str("      { error: 'Internal server error' },\n");
        code.push_str("      { status: 500 }\n");
        code.push_str("    );\n");
        code.push_str("  }\n");
        code.push_str("}\n");

        // File path: routes/user/verify/post.ts
        let file_path = format!("{}/{}", component_ref.path, method.filename());

        GeneratedFile {
            path: file_path,
            content: code,
        }
    }

    /// Generate handlers for all HTTP methods (if needed)
    pub fn generate_all_methods(
        &self,
        model_name: &str,
        field_name: &str,
        component_ref: &ComponentReference,
    ) -> Vec<GeneratedFile> {
        vec![
            self.generate_route_handler(model_name, field_name, component_ref, HttpMethod::Get),
            self.generate_route_handler(model_name, field_name, component_ref, HttpMethod::Post),
            self.generate_route_handler(model_name, field_name, component_ref, HttpMethod::Put),
            self.generate_route_handler(model_name, field_name, component_ref, HttpMethod::Delete),
        ]
    }
}
