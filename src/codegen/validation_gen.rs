use crate::ast::{ConstraintParam, Field, FieldType};

pub struct ValidationGenerator;

impl ValidationGenerator {
    pub fn new() -> Self {
        ValidationGenerator
    }

    pub fn generate_validation_functions(&self) -> String {
        let mut code = String::new();

        // Email validation function
        code.push_str(
            r#"fn validate_email(value: &str) -> Result<(), String> {
    let email_regex = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
    if !regex::Regex::new(email_regex).unwrap().is_match(value) {
        return Err(format!("'{}' is not a valid email address", value));
    }
    Ok(())
}

"#,
        );

        // URL validation function
        code.push_str(
            r#"fn validate_url(value: &str) -> Result<(), String> {
    let url_regex = r"^https?://[^\s/$.?#].[^\s]*$";
    if !regex::Regex::new(url_regex).unwrap().is_match(value) {
        return Err(format!("'{}' is not a valid URL", value));
    }
    Ok(())
}

"#,
        );

        // Pattern validation function (generic)
        code.push_str(
            r#"fn validate_pattern(value: &str, pattern: &str) -> Result<(), String> {
    if !regex::Regex::new(pattern).unwrap().is_match(value) {
        return Err(format!("'{}' does not match required pattern", value));
    }
    Ok(())
}

"#,
        );

        code
    }

    pub fn generate_field_validation(&self, field: &Field) -> String {
        let mut code = String::new();

        // Skip validation for relations
        if matches!(&field.field_type, FieldType::Relation(_)) {
            return code;
        }

        for constraint in &field.constraints {
            match constraint.name.as_str() {
                "email" => {
                    if matches!(field.field_type, FieldType::String) {
                        code.push_str(&format!("        validate_email(&{})?;\n", field.name));
                    }
                }
                "url" => {
                    if matches!(field.field_type, FieldType::String) {
                        code.push_str(&format!("        validate_url(&{})?;\n", field.name));
                    }
                }
                "min" => {
                    if let Some(ConstraintParam::Number(min_val)) = constraint.params.first() {
                        match field.field_type {
                            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => {
                                code.push_str(&format!(
                                    "        if {} < {} {{\n",
                                    field.name, min_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at least {}\".to_string());\n", field.name, min_val));
                                code.push_str("        }\n");
                            }
                            FieldType::String => {
                                // For strings, min means minimum length
                                code.push_str(&format!(
                                    "        if {}.len() < {} {{\n",
                                    field.name, min_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at least {} characters\".to_string());\n", field.name, min_val));
                                code.push_str("        }\n");
                            }
                            _ => {}
                        }
                    }
                }
                "max" => {
                    if let Some(ConstraintParam::Number(max_val)) = constraint.params.first() {
                        match field.field_type {
                            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => {
                                code.push_str(&format!(
                                    "        if {} > {} {{\n",
                                    field.name, max_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at most {}\".to_string());\n", field.name, max_val));
                                code.push_str("        }\n");
                            }
                            FieldType::String => {
                                // For strings, max means maximum length
                                code.push_str(&format!(
                                    "        if {}.len() > {} {{\n",
                                    field.name, max_val
                                ));
                                code.push_str(&format!("            return Err(\"Validation error: {} must be at most {} characters\".to_string());\n", field.name, max_val));
                                code.push_str("        }\n");
                            }
                            _ => {}
                        }
                    }
                }
                "pattern" => {
                    if let Some(ConstraintParam::String(pattern)) = constraint.params.first() {
                        if matches!(field.field_type, FieldType::String) {
                            code.push_str(&format!(
                                "        validate_pattern(&{}, \"{}\")?;\n",
                                field.name, pattern
                            ));
                        }
                    }
                }
                _ => {
                    // Unknown constraints are ignored for now
                }
            }
        }

        code
    }
}
