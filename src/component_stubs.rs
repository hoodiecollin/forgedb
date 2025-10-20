use crate::ast::{
    ComponentProtocol, ComponentReference, FieldType, RelationInclusion, Schema,
};
use crate::codegen::{ir::IrSchema, GeneratedFile};

pub struct ComponentStubGenerator;

#[derive(Debug, Clone, Copy)]
pub enum StubTemplate {
    Minimal,
    Detailed,
}

impl ComponentStubGenerator {
    pub fn new() -> Self {
        ComponentStubGenerator
    }

    /// Generate component stubs for all component fields in the schema using IR
    pub fn generate_stubs(
        &self,
        schema: &Schema,
        template: StubTemplate,
    ) -> Vec<GeneratedFile> {
        let ir_schema = IrSchema::from_ast(schema.clone());
        let mut files = vec![];

        for ir_model in &ir_schema.models {
            for ir_field in &ir_model.fields {
                if let FieldType::Component(component_ref) = &ir_field.field_type {
                    match component_ref.protocol {
                        ComponentProtocol::Tsx | ComponentProtocol::Jsx => {
                            files.push(self.generate_react_component_stub(
                                &ir_model.name,
                                &ir_field.name,
                                component_ref,
                                ir_model,
                                template,
                            ));
                        }
                        ComponentProtocol::Api => {
                            // API route handlers will be generated separately
                        }
                    }
                }
            }
        }

        files
    }

    /// Generate a React/TSX component stub using IR
    fn generate_react_component_stub(
        &self,
        model_name: &str,
        field_name: &str,
        component_ref: &ComponentReference,
        ir_model: &crate::codegen::ir::IrModel,
        template: StubTemplate,
    ) -> GeneratedFile {
        let mut code = String::new();

        // Props type name
        let props_type = format!("{}{}Props", model_name, Self::capitalize_first(field_name));

        // Import statement
        code.push_str(&format!(
            "import {{ {} }} from '../../../generated/sdk/component-props';\n\n",
            props_type
        ));

        // Component name (convert path to PascalCase)
        let component_name = format!("{}{}", model_name, Self::capitalize_first(field_name));

        // Generate component based on template
        match template {
            StubTemplate::Minimal => {
                self.generate_minimal_component(&mut code, &component_name, &props_type);
            }
            StubTemplate::Detailed => {
                self.generate_detailed_component(
                    &mut code,
                    &component_name,
                    &props_type,
                    model_name,
                    ir_model,
                    component_ref,
                );
            }
        }

        // Use page.tsx naming convention
        let file_path = format!("{}/page.tsx", component_ref.path);

        GeneratedFile {
            path: file_path,
            content: code,
        }
    }

    /// Generate minimal component stub
    fn generate_minimal_component(&self, code: &mut String, component_name: &str, props_type: &str) {
        code.push_str(&format!(
            "export default function {}({{ data, computed, relations }}: {}) {{\n",
            component_name, props_type
        ));
        code.push_str("  return (\n");
        code.push_str(&format!(
            "    <div className=\"{}\">\n",
            Self::kebab_case(component_name)
        ));
        code.push_str("      {/* TODO: Implement component */}\n");
        code.push_str("      <pre>{JSON.stringify(data, null, 2)}</pre>\n");
        code.push_str("    </div>\n");
        code.push_str("  );\n");
        code.push_str("}\n");
    }

    /// Generate detailed component stub with all fields rendered using IR
    fn generate_detailed_component(
        &self,
        code: &mut String,
        component_name: &str,
        props_type: &str,
        _model_name: &str,
        ir_model: &crate::codegen::ir::IrModel,
        component_ref: &ComponentReference,
    ) {
        code.push_str(&format!(
            "export default function {}({{ data, computed, relations }}: {}) {{\n",
            component_name, props_type
        ));
        code.push_str("  return (\n");
        code.push_str(&format!(
            "    <div className=\"{}\">\n",
            Self::kebab_case(component_name)
        ));

        // Render data fields using IR stored fields
        code.push_str("      <div className=\"data\">\n");
        for ir_field in &ir_model.stored_fields {
            code.push_str(&format!(
                "        <div className=\"field\">\n          <label>{}</label>\n          <span>{{data.{}}}</span>\n        </div>\n",
                Self::humanize(&ir_field.name),
                ir_field.name
            ));
        }
        code.push_str("      </div>\n");

        // Render computed fields using IR
        if !ir_model.computed_fields.is_empty() {
            code.push_str("\n      {computed && (\n");
            code.push_str("        <div className=\"computed\">\n");
            for ir_field in &ir_model.computed_fields {
                code.push_str(&format!(
                    "          <div className=\"field\">\n            <label>{}</label>\n            <span>{{computed.{}}}</span>\n          </div>\n",
                    Self::humanize(&ir_field.name),
                    ir_field.name
                ));
            }
            code.push_str("        </div>\n");
            code.push_str("      )}\n");
        }

        // Render relations based on @relations directive using IR
        if !matches!(component_ref.relations, RelationInclusion::None) {
            code.push_str("\n      {relations && (\n");
            code.push_str("        <div className=\"relations\">\n");

            let relation_fields: Vec<_> = match &component_ref.relations {
                RelationInclusion::All => {
                    ir_model.virtual_fields.iter().filter(|f| f.field_type.is_relation()).collect()
                }
                RelationInclusion::Specific(names) => ir_model.fields
                    .iter()
                    .filter(|f| names.contains(&f.name))
                    .collect(),
                RelationInclusion::None => vec![],
            };

            for field in relation_fields {
                if let FieldType::Relation(rel_type) = &field.field_type {
                    use crate::ast::RelationType;
                    match rel_type {
                        RelationType::OneToMany(_) | RelationType::ManyToMany(_) => {
                            code.push_str(&format!(
                                "          {{relations.{} && (\n",
                                field.name
                            ));
                            code.push_str(&format!(
                                "            <div className=\"relation-{}\">\n",
                                Self::kebab_case(&field.name)
                            ));
                            code.push_str(&format!(
                                "              <h3>{}</h3>\n",
                                Self::humanize(&field.name)
                            ));
                            code.push_str(&format!(
                                "              {{relations.{}.map((item) => (\n",
                                field.name
                            ));
                            code.push_str("                <div key={item.id}>{JSON.stringify(item)}</div>\n");
                            code.push_str("              ))}\n");
                            code.push_str("            </div>\n");
                            code.push_str("          )}\n");
                        }
                        RelationType::RequiredReference(_) | RelationType::OptionalReference(_) => {
                            code.push_str(&format!(
                                "          {{relations.{} && (\n",
                                field.name
                            ));
                            code.push_str(&format!(
                                "            <div className=\"relation-{}\">\n",
                                Self::kebab_case(&field.name)
                            ));
                            code.push_str(&format!(
                                "              <h3>{}</h3>\n",
                                Self::humanize(&field.name)
                            ));
                            code.push_str(&format!(
                                "              <div>{{JSON.stringify(relations.{})}}</div>\n",
                                field.name
                            ));
                            code.push_str("            </div>\n");
                            code.push_str("          )}\n");
                        }
                    }
                }
            }

            code.push_str("        </div>\n");
            code.push_str("      )}\n");
        }

        code.push_str("    </div>\n");
        code.push_str("  );\n");
        code.push_str("}\n");
    }

    /// Capitalize first letter
    fn capitalize_first(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().chain(chars).collect(),
        }
    }

    /// Convert to kebab-case
    fn kebab_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('-');
            }
            result.push(c.to_lowercase().next().unwrap());
        }
        result
    }

    /// Convert field name to human readable
    fn humanize(s: &str) -> String {
        let capitalized = Self::capitalize_first(s);
        capitalized.replace('_', " ")
    }
}
