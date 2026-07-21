use crate::types::SchemaChange;
use std::collections::{HashMap, HashSet};

/// Simple schema representation for diffing
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSchema {
    pub models: Vec<SimpleModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleModel {
    pub name: String,
    pub fields: Vec<SimpleField>,
    pub composite_indexes: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleField {
    pub name: String,
    pub field_type: String,
    pub nullable: bool,
    pub unique: bool,
    pub indexed: bool,
    pub index_type: String,
    pub constraints: Vec<SimpleConstraint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleConstraint {
    pub name: String,
    pub params: Vec<String>,
}

/// Schema differ - compares two schemas and generates change list
pub struct SchemaDiffer;

impl SchemaDiffer {
    /// Diff two schemas and return the list of changes
    pub fn diff(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // Build maps for easier lookup
        let old_models: HashMap<_, _> = old_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();
        let new_models: HashMap<_, _> = new_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        // Detect removed models
        for old_model_name in old_models.keys() {
            if !new_models.contains_key(old_model_name) {
                changes.push(SchemaChange::RemoveModel {
                    model_name: old_model_name.clone(),
                });
            }
        }

        // Detect added models
        for new_model_name in new_models.keys() {
            if !old_models.contains_key(new_model_name) {
                changes.push(SchemaChange::AddModel {
                    model_name: new_model_name.clone(),
                });
            }
        }

        // Detect field changes in existing models
        for (model_name, new_model) in new_models.iter() {
            if let Some(old_model) = old_models.get(model_name) {
                let field_changes = Self::diff_fields(model_name, old_model, new_model);
                changes.extend(field_changes);

                let index_changes = Self::diff_composite_indexes(model_name, old_model, new_model);
                changes.extend(index_changes);
            }
        }

        changes
    }

    /// Diff fields between two models
    fn diff_fields(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_fields: HashMap<_, _> = old_model
            .fields
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();
        let new_fields: HashMap<_, _> = new_model
            .fields
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        // Detect removed fields
        for old_field_name in old_fields.keys() {
            if !new_fields.contains_key(old_field_name) {
                changes.push(SchemaChange::RemoveField {
                    model_name: model_name.to_string(),
                    field_name: old_field_name.clone(),
                });
            }
        }

        // Detect added and modified fields
        for (field_name, new_field) in new_fields.iter() {
            if let Some(old_field) = old_fields.get(field_name) {
                // Field exists in both - check for changes

                // Type change
                if old_field.field_type != new_field.field_type {
                    changes.push(SchemaChange::ChangeFieldType {
                        model_name: model_name.to_string(),
                        field_name: field_name.clone(),
                        old_type: old_field.field_type.clone(),
                        new_type: new_field.field_type.clone(),
                    });
                }

                // Nullability change
                if old_field.nullable != new_field.nullable {
                    changes.push(SchemaChange::ChangeFieldNullability {
                        model_name: model_name.to_string(),
                        field_name: field_name.clone(),
                        old_nullable: old_field.nullable,
                        new_nullable: new_field.nullable,
                    });
                }

                // Index changes
                if old_field.indexed != new_field.indexed {
                    if new_field.indexed {
                        changes.push(SchemaChange::AddIndex {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                            index_type: new_field.index_type.clone(),
                        });
                    } else {
                        changes.push(SchemaChange::RemoveIndex {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                        });
                    }
                }

                // Unique constraint changes
                if old_field.unique != new_field.unique {
                    if new_field.unique {
                        changes.push(SchemaChange::AddUniqueConstraint {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                        });
                    } else {
                        changes.push(SchemaChange::RemoveUniqueConstraint {
                            model_name: model_name.to_string(),
                            field_name: field_name.clone(),
                        });
                    }
                }

                // Constraint changes
                let constraint_changes =
                    Self::diff_constraints(model_name, field_name, old_field, new_field);
                changes.extend(constraint_changes);
            } else {
                // New field
                changes.push(SchemaChange::AddField {
                    model_name: model_name.to_string(),
                    field_name: field_name.clone(),
                    field_type: new_field.field_type.clone(),
                    nullable: new_field.nullable,
                    default_value: None, // Could be enhanced to detect defaults
                });
            }
        }

        changes
    }

    /// Diff constraints between two fields
    fn diff_constraints(
        model_name: &str,
        field_name: &str,
        old_field: &SimpleField,
        new_field: &SimpleField,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_constraints: HashMap<_, _> = old_field
            .constraints
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        let new_constraints: HashMap<_, _> = new_field
            .constraints
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        // Removed constraints
        for (constraint_name, _) in old_constraints.iter() {
            if !new_constraints.contains_key(constraint_name) {
                changes.push(SchemaChange::RemoveConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                });
            }
        }

        // Added constraints
        for (constraint_name, constraint) in new_constraints.iter() {
            if !old_constraints.contains_key(constraint_name) {
                changes.push(SchemaChange::AddConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                    constraint_params: constraint.params.clone(),
                });
            }
        }

        changes
    }

    /// Diff composite indexes
    fn diff_composite_indexes(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // Convert to sets for comparison
        let old_indexes: HashSet<_> = old_model
            .composite_indexes
            .iter()
            .map(|idx| idx.clone())
            .collect();
        let new_indexes: HashSet<_> = new_model
            .composite_indexes
            .iter()
            .map(|idx| idx.clone())
            .collect();

        // Removed indexes
        for old_idx in old_indexes.iter() {
            if !new_indexes.contains(old_idx) {
                changes.push(SchemaChange::RemoveCompositeIndex {
                    model_name: model_name.to_string(),
                    fields: old_idx.clone(),
                });
            }
        }

        // Added indexes
        for new_idx in new_indexes.iter() {
            if !old_indexes.contains(new_idx) {
                changes.push(SchemaChange::AddCompositeIndex {
                    model_name: model_name.to_string(),
                    fields: new_idx.clone(),
                });
            }
        }

        changes
    }
}
