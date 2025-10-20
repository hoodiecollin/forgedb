//! Request validation generation
//!
//! Generates per-model validator functions for Create/Update requests
//! using the centralized constraints module.

use crate::codegen::{constraints, ir::IrModel, naming};
use proc_macro2::TokenStream;
use quote::quote;

/// Generate validation functions for a model
pub fn generate_validation_functions(ir_model: &IrModel) -> TokenStream {
    let create_validator = generate_create_validator(ir_model);
    let update_validator = generate_update_validator(ir_model);
    
    quote! {
        #create_validator
        
        #update_validator
    }
}

/// Generate a validation function for create requests
fn generate_create_validator(ir_model: &IrModel) -> TokenStream {
    let model_name = syn::Ident::new(&ir_model.name, proc_macro2::Span::call_site());
    let fn_name = syn::Ident::new(
        &format!("validate_create_{}", naming::to_snake_case(&ir_model.name)),
        proc_macro2::Span::call_site(),
    );
    
    let mut validations = Vec::new();
    
    // Generate validation for each create request field
    for ir_field in ir_model.create_request_fields() {
        if ir_field.original.constraints.is_empty() {
            continue;
        }
        
        let field_name = &ir_field.name;
        let field_name_ident = syn::Ident::new(field_name, proc_macro2::Span::call_site());
        
        let field_validations = constraints::get_rust_validations(
            &ir_field.original.constraints,
            &ir_field.field_type,
        );
        
        for (idx, validation_expr) in field_validations.iter().enumerate() {
            // Parse the validation expression into a token stream
            let validation_tokens: TokenStream = validation_expr.parse().unwrap_or_else(|_| {
                quote! { true }
            });
            
            let error_msg = format!(
                "Validation failed for field '{}' (constraint {})",
                field_name, idx
            );
            
            // For String fields, we need to use the field directly
            // For numeric fields, we need to reference them
            let check = if matches!(ir_field.field_type, crate::ast::FieldType::String) {
                quote! {
                    {
                        let value = &request.#field_name_ident;
                        if !(#validation_tokens) {
                            return Err(#error_msg.to_string());
                        }
                    }
                }
            } else {
                quote! {
                    {
                        let value = &request.#field_name_ident;
                        if !(#validation_tokens) {
                            return Err(#error_msg.to_string());
                        }
                    }
                }
            };
            
            validations.push(check);
        }
    }
    
    let create_type_name = syn::Ident::new(
        &format!("Create{}", ir_model.name),
        proc_macro2::Span::call_site(),
    );
    
    if validations.is_empty() {
        // No validations needed
        return quote! {
            #[allow(unused_variables)]
            pub fn #fn_name(request: &#create_type_name) -> Result<(), String> {
                Ok(())
            }
        };
    }
    
    quote! {
        pub fn #fn_name(request: &#create_type_name) -> Result<(), String> {
            #(#validations)*
            Ok(())
        }
    }
}

/// Generate a validation function for update requests
fn generate_update_validator(ir_model: &IrModel) -> TokenStream {
    let model_name = syn::Ident::new(&ir_model.name, proc_macro2::Span::call_site());
    let fn_name = syn::Ident::new(
        &format!("validate_update_{}", naming::to_snake_case(&ir_model.name)),
        proc_macro2::Span::call_site(),
    );
    
    let mut validations = Vec::new();
    
    // Generate validation for each update request field (all are optional)
    for ir_field in ir_model.update_request_fields() {
        if ir_field.original.constraints.is_empty() {
            continue;
        }
        
        let field_name = &ir_field.name;
        let field_name_ident = syn::Ident::new(field_name, proc_macro2::Span::call_site());
        
        let field_validations = constraints::get_rust_validations(
            &ir_field.original.constraints,
            &ir_field.field_type,
        );
        
        for (idx, validation_expr) in field_validations.iter().enumerate() {
            let validation_tokens: TokenStream = validation_expr.parse().unwrap_or_else(|_| {
                quote! { true }
            });
            
            let error_msg = format!(
                "Validation failed for field '{}' (constraint {})",
                field_name, idx
            );
            
            // For update requests, fields are optional, so check if present first
            let check = quote! {
                if let Some(value) = &request.#field_name_ident {
                    if !(#validation_tokens) {
                        return Err(#error_msg.to_string());
                    }
                }
            };
            
            validations.push(check);
        }
    }
    
    let update_type_name = syn::Ident::new(
        &format!("Update{}", ir_model.name),
        proc_macro2::Span::call_site(),
    );
    
    if validations.is_empty() {
        // No validations needed
        return quote! {
            #[allow(unused_variables)]
            pub fn #fn_name(request: &#update_type_name) -> Result<(), String> {
                Ok(())
            }
        };
    }
    
    quote! {
        pub fn #fn_name(request: &#update_type_name) -> Result<(), String> {
            #(#validations)*
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Constraint, ConstraintParam, Field, FieldType, IndexType, Model};
    
    fn create_test_field(name: &str, field_type: FieldType, constraints: Vec<Constraint>) -> Field {
        Field {
            name: name.to_string(),
            field_type,
            auto_generate: false,
            unique: false,
            indexed: false,
            constraints,
            index_type: IndexType::Hash,
            is_computed: false,
            fulltext_indexed: false,
            is_materialized: false,
        }
    }
    
    #[test]
    fn test_generate_validation_functions() {
        let model = Model {
            name: "User".to_string(),
            fields: vec![
                create_test_field("id", FieldType::Uuid, vec![]),
                create_test_field(
                    "email",
                    FieldType::String,
                    vec![Constraint {
                        name: "email".to_string(),
                        params: vec![],
                    }],
                ),
            ],
            composite_indexes: vec![],
            soft_delete: false,
        };
        
        let ir_model = IrModel::from_ast(model);
        let validations = generate_validation_functions(&ir_model);
        
        let code = validations.to_string();
        assert!(code.contains("validate_create_user"));
        assert!(code.contains("validate_update_user"));
    }
}
