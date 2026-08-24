use crate::types::SchemaChange;
use std::collections::{HashMap, HashSet};
use std::collections::BTreeMap;

/// Simple schema representation for diffing.
///
/// # Why `enums` and `structs` are here and not only `models` (#438)
///
/// A field's *type* is compared as a string, and for an `enum`/`struct` field
/// that string carries only the **name** (`Enum("Status")`) — the definition
/// lives beside the models, not inside the reference. So a schema that reorders
/// `Status`'s variants produces two `SimpleSchema` values whose models compare
/// **equal**, and the differ, handed two equal values, correctly reports
/// nothing.
///
/// That silence is not cosmetic. An enum is stored as a **positional 1-byte
/// discriminant** (variants map to `0..N` in declaration order), so a reorder
/// re-maps every already-written row to a different variant with no byte on
/// disk changing and no version moving; an inline `struct` is `#[repr(C)]` and
/// every field's offset is a function of the whole declaration. Carrying the
/// ordered definitions here is what lets the differ *see* those edits, record a
/// hop, and bump the schema version that arms the generated open guard.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSchema {
    pub models: Vec<SimpleModel>,
    /// Every declared `enum`, with its variants in **declaration order**. The
    /// order is the payload: it is the byte→meaning map.
    pub enums: Vec<SimpleEnum>,
    /// Every declared inline `struct`, with its fields in **declaration
    /// order**. Order and per-field width are both load-bearing on disk.
    pub structs: Vec<SimpleStruct>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleModel {
    pub name: String,
    pub fields: Vec<SimpleField>,
    pub composite_indexes: Vec<Vec<String>>,
}

/// A declared `enum`, projected for diffing (#438).
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleEnum {
    pub name: String,
    /// Declaration order — this IS the stored discriminant mapping.
    pub variants: Vec<String>,
}

/// A declared inline `struct`, projected for diffing (#438).
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleStruct {
    pub name: String,
    /// Declaration order — this IS the `#[repr(C)]` field layout.
    pub fields: Vec<SimpleField>,
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
    /// The names of every `enum` / `struct` this field's type reaches
    /// **transitively** (#438).
    ///
    /// This is how a definition change is projected back onto the model fields
    /// that store it. Transitivity is the whole point and is easy to get wrong:
    /// an enum can sit inside an inline struct, and a struct inside a struct
    /// (`[Point; 4]` inside `Shape`, `Color` inside `Point`). When `Color`
    /// changes, the outer field's own declaration text does not — so a
    /// dependency list that stops at depth one is the same blindness wearing a
    /// smaller hat.
    ///
    /// Populated where the full AST is (the CLI's `to_simple_schema`), so this
    /// crate stays a pure comparison over its own value types with no parser
    /// dependency.
    pub depends_on: Vec<String>,
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

        // #438: the definitions themselves. A field's type string carries only
        // the enum/struct NAME, so these edits are invisible to `diff_fields`
        // above by construction — nothing there is wrong, it is comparing two
        // values that are equal.
        changes.extend(Self::diff_enums(old_schema, new_schema));
        changes.extend(Self::diff_structs(old_schema, new_schema));

        changes
    }

    /// Diff the declared `enum`s and project each change onto the model fields
    /// that store it (#438).
    ///
    /// Only an enum present on **both** sides is diffed. A newly-declared enum
    /// has no stored rows, and a removed one is already reported through the
    /// referencing field (which must itself have changed type or gone away).
    fn diff_enums(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_enums: BTreeMap<_, _> = old_schema
            .enums
            .iter()
            .map(|e| (e.name.clone(), e))
            .collect();
        let new_enums: BTreeMap<_, _> = new_schema
            .enums
            .iter()
            .map(|e| (e.name.clone(), e))
            .collect();

        for (name, new_enum) in new_enums.iter() {
            let Some(old_enum) = old_enums.get(name) else {
                continue;
            };
            if old_enum.variants == new_enum.variants {
                continue;
            }
            for (model_name, field_name) in Self::dependent_fields(old_schema, new_schema, name) {
                changes.push(SchemaChange::ChangeEnumVariants {
                    model_name,
                    field_name,
                    enum_name: name.clone(),
                    old_variants: old_enum.variants.clone(),
                    new_variants: new_enum.variants.clone(),
                });
            }
        }

        changes
    }

    /// Diff the declared inline `struct`s and project each change onto the model
    /// fields that store one (#438).
    ///
    /// A struct's layout is `(name, type)` per field, in declaration order —
    /// both halves are load-bearing on disk, so both are carried.
    fn diff_structs(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange> {
        let mut changes = Vec::new();

        let old_structs: BTreeMap<_, _> = old_schema
            .structs
            .iter()
            .map(|s| (s.name.clone(), s))
            .collect();
        let new_structs: BTreeMap<_, _> = new_schema
            .structs
            .iter()
            .map(|s| (s.name.clone(), s))
            .collect();

        for (name, new_struct) in new_structs.iter() {
            let Some(old_struct) = old_structs.get(name) else {
                continue;
            };
            let old_fields = Self::layout(&old_struct.fields);
            let new_fields = Self::layout(&new_struct.fields);
            if old_fields == new_fields {
                continue;
            }
            for (model_name, field_name) in Self::dependent_fields(old_schema, new_schema, name) {
                changes.push(SchemaChange::ChangeStructLayout {
                    model_name,
                    field_name,
                    struct_name: name.clone(),
                    old_fields: old_fields.clone(),
                    new_fields: new_fields.clone(),
                });
            }
        }

        changes
    }

    /// A struct's on-disk layout as `(field name, field type)` in declaration order.
    fn layout(fields: &[SimpleField]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|f| (f.name.clone(), f.field_type.clone()))
            .collect()
    }

    /// Every `(model, field)` that stores a value reaching `type_name`, in a
    /// deterministic order (#438).
    ///
    /// Restricted to models AND fields present on **both** sides: a newly-added
    /// model or field has no rows written under the old mapping, so there is
    /// nothing about it to migrate, and a removed one is already reported as its
    /// own change.
    fn dependent_fields(
        old_schema: &SimpleSchema,
        new_schema: &SimpleSchema,
        type_name: &str,
    ) -> Vec<(String, String)> {
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

        let mut out = Vec::new();
        for (model_name, new_model) in new_models.iter() {
            let Some(old_model) = old_models.get(model_name) else {
                continue;
            };
            let old_field_names: HashSet<&String> =
                old_model.fields.iter().map(|f| &f.name).collect();
            let mut fields: Vec<&SimpleField> = new_model
                .fields
                .iter()
                .filter(|f| old_field_names.contains(&f.name))
                .filter(|f| f.depends_on.iter().any(|d| d == type_name))
                .collect();
            fields.sort_by(|a, b| a.name.cmp(&b.name));
            for f in fields {
                out.push((model_name.clone(), f.name.clone()));
            }
        }
        out
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
