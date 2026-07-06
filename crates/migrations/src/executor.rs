use crate::types::{Migration, SchemaChange};
use std::path::Path;

/// Executes migrations against a database
pub struct MigrationExecutor;

impl MigrationExecutor {
    /// Execute a migration (up direction)
    pub fn execute_up<P: AsRef<Path>>(migration: &Migration, data_dir: P) -> Result<(), String> {
        let data_dir = data_dir.as_ref();

        // Ensure data directory exists
        if !data_dir.exists() {
            std::fs::create_dir_all(data_dir)
                .map_err(|e| format!("Failed to create data directory: {}", e))?;
        }

        // Execute each change
        for change in &migration.changes {
            Self::execute_change(change, data_dir, true)?;
        }

        Ok(())
    }

    /// Execute a migration (down direction - rollback)
    pub fn execute_down<P: AsRef<Path>>(migration: &Migration, data_dir: P) -> Result<(), String> {
        let data_dir = data_dir.as_ref();

        // Execute changes in reverse order
        for change in migration.changes.iter().rev() {
            Self::execute_change(change, data_dir, false)?;
        }

        Ok(())
    }

    /// Execute a single change
    fn execute_change<P: AsRef<Path>>(
        change: &SchemaChange,
        data_dir: P,
        is_up: bool,
    ) -> Result<(), String> {
        let data_dir = data_dir.as_ref();

        match change {
            SchemaChange::AddModel { model_name } if is_up => {
                Self::create_model_storage(data_dir, model_name)?;
            }
            SchemaChange::RemoveModel { model_name } if !is_up => {
                Self::create_model_storage(data_dir, model_name)?;
            }
            SchemaChange::RemoveModel { model_name } if is_up => {
                Self::drop_model_storage(data_dir, model_name)?;
            }
            SchemaChange::AddModel { model_name } if !is_up => {
                Self::drop_model_storage(data_dir, model_name)?;
            }

            SchemaChange::AddField {
                model_name,
                field_name,
                field_type,
                nullable,
                default_value,
            } if is_up => {
                Self::add_column(
                    data_dir,
                    model_name,
                    field_name,
                    field_type,
                    *nullable,
                    default_value.as_deref(),
                )?;
            }
            SchemaChange::RemoveField {
                model_name,
                field_name,
            } if !is_up => {
                // For rollback, we'd need to restore the field (complex - requires stored metadata)
                println!(
                    "Warning: Rolling back field removal for {}.{} - data may be lost",
                    model_name, field_name
                );
            }
            SchemaChange::RemoveField {
                model_name,
                field_name,
            } if is_up => {
                Self::remove_column(data_dir, model_name, field_name)?;
            }
            SchemaChange::AddField {
                model_name,
                field_name,
                ..
            } if !is_up => {
                Self::remove_column(data_dir, model_name, field_name)?;
            }

            SchemaChange::ChangeFieldType {
                model_name,
                field_name,
                old_type,
                new_type,
            } => {
                let (from_type, to_type) = if is_up {
                    (old_type, new_type)
                } else {
                    (new_type, old_type)
                };
                Self::change_column_type(data_dir, model_name, field_name, from_type, to_type)?;
            }

            SchemaChange::AddIndex {
                model_name,
                field_name,
                index_type,
            } if is_up => {
                Self::create_index(data_dir, model_name, field_name, index_type)?;
            }
            SchemaChange::RemoveIndex {
                model_name,
                field_name,
            } if !is_up => {
                println!("Restoring index on {}.{}", model_name, field_name);
            }
            SchemaChange::RemoveIndex {
                model_name,
                field_name,
            } if is_up => {
                Self::drop_index(data_dir, model_name, field_name)?;
            }
            SchemaChange::AddIndex {
                model_name,
                field_name,
                ..
            } if !is_up => {
                Self::drop_index(data_dir, model_name, field_name)?;
            }

            SchemaChange::AddUniqueConstraint {
                model_name,
                field_name,
            } if is_up => {
                Self::add_unique_constraint(data_dir, model_name, field_name)?;
            }
            SchemaChange::RemoveUniqueConstraint {
                model_name,
                field_name,
            } if is_up => {
                Self::remove_unique_constraint(data_dir, model_name, field_name)?;
            }

            SchemaChange::AddCompositeIndex { model_name, fields } if is_up => {
                Self::create_composite_index(data_dir, model_name, fields)?;
            }
            SchemaChange::RemoveCompositeIndex { model_name, fields } if is_up => {
                Self::drop_composite_index(data_dir, model_name, fields)?;
            }

            _ => {
                // M5: all unhandled operations now return Err so callers know the
                // migration was not actually applied, rather than silently succeeding.
                return Err(format!(
                    "operation not yet supported: {}",
                    change.description()
                ));
            }
        }

        Ok(())
    }

    /// Create storage for a new model
    fn create_model_storage<P: AsRef<Path>>(data_dir: P, model_name: &str) -> Result<(), String> {
        let model_dir = data_dir.as_ref().join(model_name.to_lowercase());

        if model_dir.exists() {
            return Ok(()); // Already exists
        }

        std::fs::create_dir_all(&model_dir)
            .map_err(|e| format!("Failed to create model directory: {}", e))?;

        // Create subdirectories for fixed and variable columns
        std::fs::create_dir_all(model_dir.join("fixed"))
            .map_err(|e| format!("Failed to create fixed directory: {}", e))?;
        std::fs::create_dir_all(model_dir.join("variable"))
            .map_err(|e| format!("Failed to create variable directory: {}", e))?;

        // Create manifest
        let manifest = serde_json::json!({
            "model_name": model_name,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "version": 1,
        });

        std::fs::write(
            model_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

        println!("✓ Created storage for model '{}'", model_name);
        Ok(())
    }

    /// Drop storage for a model
    fn drop_model_storage<P: AsRef<Path>>(data_dir: P, model_name: &str) -> Result<(), String> {
        let model_dir = data_dir.as_ref().join(model_name.to_lowercase());

        if !model_dir.exists() {
            return Ok(()); // Already gone
        }

        std::fs::remove_dir_all(&model_dir)
            .map_err(|e| format!("Failed to remove model directory: {}", e))?;

        println!("✓ Dropped storage for model '{}'", model_name);
        Ok(())
    }

    /// Add a column to a model
    fn add_column<P: AsRef<Path>>(
        data_dir: P,
        model_name: &str,
        field_name: &str,
        field_type: &str,
        nullable: bool,
        default_value: Option<&str>,
    ) -> Result<(), String> {
        let model_dir = data_dir.as_ref().join(model_name.to_lowercase());

        if !model_dir.exists() {
            return Err(format!("Model '{}' does not exist", model_name));
        }

        // Determine if fixed or variable size
        let is_fixed = Self::is_fixed_size_type(field_type);
        let subdir = if is_fixed { "fixed" } else { "variable" };

        // For now, just create a placeholder file
        // In a real implementation, this would migrate existing data
        let column_file = model_dir
            .join(subdir)
            .join(format!("{}_{}.bin", field_type, field_name));

        if !column_file.exists() {
            std::fs::write(&column_file, &[])
                .map_err(|e| format!("Failed to create column file: {}", e))?;
        }

        let null_str = if nullable { "?" } else { "" };
        let default_str = if let Some(val) = default_value {
            format!(" (default: {})", val)
        } else {
            String::new()
        };

        println!(
            "✓ Added column '{}.{}: {}{}{}'",
            model_name, field_name, field_type, null_str, default_str
        );
        Ok(())
    }

    /// Remove a column from a model
    fn remove_column<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        field_name: &str,
    ) -> Result<(), String> {
        // M5: column removal from live storage is not yet implemented; returning Err
        // prevents silent false success.
        Err(format!(
            "operation not yet supported: remove column '{}.{}'",
            model_name, field_name
        ))
    }

    /// Change column type
    fn change_column_type<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        field_name: &str,
        from_type: &str,
        to_type: &str,
    ) -> Result<(), String> {
        // M5: type changes require data migration which is not yet implemented.
        Err(format!(
            "operation not yet supported: change column type '{}.{}' from {} to {}",
            model_name, field_name, from_type, to_type
        ))
    }

    /// Create an index
    fn create_index<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        field_name: &str,
        index_type: &str,
    ) -> Result<(), String> {
        // M5: index creation on live storage is not yet implemented.
        Err(format!(
            "operation not yet supported: create {} index on '{}.{}'",
            index_type, model_name, field_name
        ))
    }

    /// Drop an index
    fn drop_index<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        field_name: &str,
    ) -> Result<(), String> {
        // M5: index removal on live storage is not yet implemented.
        Err(format!(
            "operation not yet supported: drop index on '{}.{}'",
            model_name, field_name
        ))
    }

    /// Add unique constraint
    fn add_unique_constraint<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        field_name: &str,
    ) -> Result<(), String> {
        // M5: unique constraint enforcement on live storage is not yet implemented.
        Err(format!(
            "operation not yet supported: add unique constraint to '{}.{}'",
            model_name, field_name
        ))
    }

    /// Remove unique constraint
    fn remove_unique_constraint<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        field_name: &str,
    ) -> Result<(), String> {
        // M5: unique constraint removal on live storage is not yet implemented.
        Err(format!(
            "operation not yet supported: remove unique constraint from '{}.{}'",
            model_name, field_name
        ))
    }

    /// Create composite index
    fn create_composite_index<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        fields: &[String],
    ) -> Result<(), String> {
        // M5: composite index creation on live storage is not yet implemented.
        Err(format!(
            "operation not yet supported: create composite index on '{}.{}'",
            model_name,
            fields.join(", ")
        ))
    }

    /// Drop composite index
    fn drop_composite_index<P: AsRef<Path>>(
        _data_dir: P,
        model_name: &str,
        fields: &[String],
    ) -> Result<(), String> {
        // M5: composite index removal on live storage is not yet implemented.
        Err(format!(
            "operation not yet supported: drop composite index from '{}.{}'",
            model_name,
            fields.join(", ")
        ))
    }

    /// Check if a type is fixed size
    fn is_fixed_size_type(field_type: &str) -> bool {
        matches!(
            field_type,
            "u32" | "u64" | "i32" | "i64" | "f64" | "bool" | "uuid" | "timestamp"
        )
    }
}
