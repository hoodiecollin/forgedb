use crate::types::SchemaChange;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeMap;

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
    /// Diff two schemas and return the list of changes.
    ///
    /// The returned list is sorted deterministically so that checksums are stable
    /// across runs (M2).
    pub fn diff(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // Build ordered maps (BTreeMap) for deterministic iteration (M2)
        let old_models: BTreeMap<_, _> = old_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();
        let new_models: BTreeMap<_, _> = new_schema
            .models
            .iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        // H4: Detect model renames — when exactly one model is removed and one is
        // added AND both have the same set of field names with matching types, treat
        // this as a rename rather than a drop+recreate to avoid data loss.
        let removed_model_names: Vec<_> = old_models
            .keys()
            .filter(|k| !new_models.contains_key(*k))
            .cloned()
            .collect();
        let added_model_names: Vec<_> = new_models
            .keys()
            .filter(|k| !old_models.contains_key(*k))
            .cloned()
            .collect();

        // Track which models were resolved as renames so they are not also emitted
        // as add/remove.
        let mut renamed_old: HashSet<String> = HashSet::new();
        let mut renamed_new: HashSet<String> = HashSet::new();

        if removed_model_names.len() == 1 && added_model_names.len() == 1 {
            let old_name = &removed_model_names[0];
            let new_name = &added_model_names[0];
            let old_model = old_models[old_name];
            let new_model = new_models[new_name];
            if Self::models_structurally_equal(old_model, new_model) {
                changes.push(SchemaChange::RenameModel {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                });
                renamed_old.insert(old_name.clone());
                renamed_new.insert(new_name.clone());
            }
        }

        // Detect removed models (M2: sorted by key via BTreeMap)
        for old_model_name in old_models.keys() {
            if !new_models.contains_key(old_model_name) && !renamed_old.contains(old_model_name) {
                changes.push(SchemaChange::RemoveModel {
                    model_name: old_model_name.clone(),
                });
            }
        }

        // Detect added models
        for new_model_name in new_models.keys() {
            if !old_models.contains_key(new_model_name) && !renamed_new.contains(new_model_name) {
                changes.push(SchemaChange::AddModel {
                    model_name: new_model_name.clone(),
                });
            }
        }

        // Detect field changes in existing models (M2: sorted by model name via BTreeMap)
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

    /// Return true when two models have the same set of field names and matching types.
    /// Used for rename detection (H4).
    fn models_structurally_equal(old: &SimpleModel, new: &SimpleModel) -> bool {
        if old.fields.len() != new.fields.len() {
            return false;
        }
        let old_map: HashMap<_, _> = old.fields.iter().map(|f| (&f.name, &f.field_type)).collect();
        let new_map: HashMap<_, _> = new.fields.iter().map(|f| (&f.name, &f.field_type)).collect();
        old_map == new_map
    }

    /// Diff fields between two models.
    ///
    /// Uses BTreeMap for deterministic ordering (M2).
    /// Detects field renames when exactly one field is removed and one is added
    /// with the same type (H4).
    fn diff_fields(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // M2: BTreeMap gives stable, sorted iteration
        let old_fields: BTreeMap<_, _> = old_model
            .fields
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();
        let new_fields: BTreeMap<_, _> = new_model
            .fields
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        let removed_field_names: Vec<_> = old_fields
            .keys()
            .filter(|k| !new_fields.contains_key(*k))
            .cloned()
            .collect();
        let added_field_names: Vec<_> = new_fields
            .keys()
            .filter(|k| !old_fields.contains_key(*k))
            .cloned()
            .collect();

        // H4: single unambiguous field rename — one removed, one added, same type
        let mut renamed_old_fields: HashSet<String> = HashSet::new();
        let mut renamed_new_fields: HashSet<String> = HashSet::new();

        if removed_field_names.len() == 1 && added_field_names.len() == 1 {
            let old_name = &removed_field_names[0];
            let new_name = &added_field_names[0];
            if old_fields[old_name].field_type == new_fields[new_name].field_type {
                changes.push(SchemaChange::RenameField {
                    model_name: model_name.to_string(),
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                });
                renamed_old_fields.insert(old_name.clone());
                renamed_new_fields.insert(new_name.clone());
            }
        }

        // Detect removed fields (M2: sorted via BTreeMap)
        for old_field_name in old_fields.keys() {
            if !new_fields.contains_key(old_field_name)
                && !renamed_old_fields.contains(old_field_name)
            {
                changes.push(SchemaChange::RemoveField {
                    model_name: model_name.to_string(),
                    field_name: old_field_name.clone(),
                });
            }
        }

        // Detect added and modified fields (M2: sorted via BTreeMap)
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
            } else if !renamed_new_fields.contains(field_name) {
                // New field
                changes.push(SchemaChange::AddField {
                    model_name: model_name.to_string(),
                    field_name: field_name.clone(),
                    field_type: new_field.field_type.clone(),
                    nullable: new_field.nullable,
                    default_value: None,
                });
            }
        }

        changes
    }

    /// Diff constraints between two fields.
    ///
    /// Uses BTreeMap for deterministic ordering (M2).
    /// Compares constraint *params* as well as names so that `@min(10)` → `@min(20)`
    /// is detected as a change (M3).
    fn diff_constraints(
        model_name: &str,
        field_name: &str,
        old_field: &SimpleField,
        new_field: &SimpleField,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // M2: BTreeMap for deterministic ordering
        let old_constraints: BTreeMap<_, _> = old_field
            .constraints
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        let new_constraints: BTreeMap<_, _> = new_field
            .constraints
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        // Removed constraints (sorted by name)
        for (constraint_name, _) in old_constraints.iter() {
            if !new_constraints.contains_key(constraint_name) {
                changes.push(SchemaChange::RemoveConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                });
            }
        }

        // Added or changed constraints (sorted by name)
        for (constraint_name, new_constraint) in new_constraints.iter() {
            if let Some(old_constraint) = old_constraints.get(constraint_name) {
                // M3: detect param changes (e.g. @min(10) → @min(20))
                if old_constraint.params != new_constraint.params {
                    // Emit a remove + add pair to represent the param update
                    changes.push(SchemaChange::RemoveConstraint {
                        model_name: model_name.to_string(),
                        field_name: field_name.to_string(),
                        constraint_name: constraint_name.clone(),
                    });
                    changes.push(SchemaChange::AddConstraint {
                        model_name: model_name.to_string(),
                        field_name: field_name.to_string(),
                        constraint_name: constraint_name.clone(),
                        constraint_params: new_constraint.params.clone(),
                    });
                }
            } else {
                changes.push(SchemaChange::AddConstraint {
                    model_name: model_name.to_string(),
                    field_name: field_name.to_string(),
                    constraint_name: constraint_name.clone(),
                    constraint_params: new_constraint.params.clone(),
                });
            }
        }

        changes
    }

    /// Diff composite indexes.
    ///
    /// Iterates in sorted order for deterministic change output (M2).
    fn diff_composite_indexes(
        model_name: &str,
        old_model: &SimpleModel,
        new_model: &SimpleModel,
    ) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        // Convert to HashSets for membership testing, but collect results in
        // sorted order (M2) by converting from a sorted BTreeSet-like iteration.
        let old_indexes: HashSet<_> = old_model.composite_indexes.iter().cloned().collect();
        let new_indexes: HashSet<_> = new_model.composite_indexes.iter().cloned().collect();

        // Removed indexes — sort for determinism
        let mut removed: Vec<_> = old_indexes.iter().filter(|idx| !new_indexes.contains(*idx)).collect();
        removed.sort();
        for old_idx in removed {
            changes.push(SchemaChange::RemoveCompositeIndex {
                model_name: model_name.to_string(),
                fields: old_idx.clone(),
            });
        }

        // Added indexes — sort for determinism
        let mut added: Vec<_> = new_indexes.iter().filter(|idx| !old_indexes.contains(*idx)).collect();
        added.sort();
        for new_idx in added {
            changes.push(SchemaChange::AddCompositeIndex {
                model_name: model_name.to_string(),
                fields: new_idx.clone(),
            });
        }

        changes
    }
}
