//! ForgeDB Migrations
//!
//! Database schema migration generation, tracking, and execution for ForgeDB.
//!
//! # Overview
//!
//! This crate provides a complete migration system for ForgeDB, enabling safe schema
//! evolution over time. It handles:
//!
//! - **Schema diffing** - Compare old and new schemas to detect changes
//! - **Migration generation** - Create migration files from schema differences
//! - **Migration tracking** - Track which migrations have been applied
//! - **Migration execution** - Apply or rollback migrations safely
//!
//! # Architecture
//!
//! The migration system consists of four main components:
//!
//! 1. **SchemaDiffer** - Compares schemas and identifies changes
//! 2. **MigrationGenerator** - Generates migration files from diffs
//! 3. **MigrationTracker** - Tracks applied migrations
//! 4. **MigrationExecutor** - Applies or rolls back migrations
//!
//! ## Migration Flow
//!
//! ```text
//! Schema v1 → Schema v2
//!     ↓
//! SchemaDiffer (detect changes)
//!     ↓
//! Migration { up, down }
//!     ↓
//! MigrationExecutor (apply changes)
//!     ↓
//! Database updated
//! ```
//!
//! # Examples
//!
//! ## Generating a Migration
//!
//! ```rust,no_run
//! use forgedb_migrations::{SchemaDiffer, MigrationGenerator, SimpleSchema};
//!
//! // Load old and new schemas
//! let old_schema = SimpleSchema::from_file("./old_schema.fdb")?;
//! let new_schema = SimpleSchema::from_file("./new_schema.fdb")?;
//!
//! // Diff schemas
//! let differ = SchemaDiffer::new();
//! let changes = differ.diff(&old_schema, &new_schema)?;
//!
//! // Generate migration
//! let generator = MigrationGenerator::new("./migrations");
//! let migration = generator.generate("add_user_age_field", changes)?;
//!
//! println!("Generated migration: {}", migration.name);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Applying Migrations
//!
//! ```rust,no_run
//! use forgedb_migrations::{MigrationExecutor, MigrationTracker};
//!
//! // Create executor and tracker
//! let mut executor = MigrationExecutor::new("./data");
//! let tracker = MigrationTracker::new("./data");
//!
//! // Get pending migrations
//! let pending = tracker.get_pending_migrations("./migrations")?;
//!
//! // Apply migrations
//! for migration in pending {
//!     println!("Applying migration: {}", migration.name);
//!     executor.apply_migration(&migration)?;
//!     tracker.mark_applied(&migration)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Rolling Back a Migration
//!
//! ```rust,no_run
//! use forgedb_migrations::{MigrationExecutor, MigrationTracker};
//!
//! let mut executor = MigrationExecutor::new("./data");
//! let tracker = MigrationTracker::new("./data");
//!
//! // Get last applied migration
//! if let Some(last) = tracker.get_last_applied()? {
//!     println!("Rolling back: {}", last.name);
//!     executor.rollback_migration(&last)?;
//!     tracker.mark_rolled_back(&last)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Detecting Schema Changes
//!
//! ```rust,no_run
//! use forgedb_migrations::{SchemaDiffer, SimpleSchema, SimpleModel, SimpleField};
//!
//! let differ = SchemaDiffer::new();
//!
//! // Create schemas
//! let old_schema = SimpleSchema {
//!     models: vec![
//!         SimpleModel {
//!             name: "User".to_string(),
//!             fields: vec![
//!                 SimpleField {
//!                     name: "id".to_string(),
//!                     field_type: "uuid".to_string(),
//!                     nullable: false,
//!                 },
//!             ],
//!         },
//!     ],
//! };
//!
//! let new_schema = SimpleSchema {
//!     models: vec![
//!         SimpleModel {
//!             name: "User".to_string(),
//!             fields: vec![
//!                 SimpleField {
//!                     name: "id".to_string(),
//!                     field_type: "uuid".to_string(),
//!                     nullable: false,
//!                 },
//!                 SimpleField {
//!                     name: "email".to_string(),
//!                     field_type: "string".to_string(),
//!                     nullable: false,
//!                 },
//!             ],
//!         },
//!     ],
//! };
//!
//! let changes = differ.diff(&old_schema, &new_schema)?;
//! println!("Detected {} changes", changes.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`SchemaDiffer`] - Schema comparison and change detection
//! - [`MigrationGenerator`] - Migration file generation
//! - [`MigrationExecutor`] - Migration application and rollback
//! - [`MigrationTracker`] - Track applied migrations
//!
//! ## Schema Representation
//!
//! - [`SimpleSchema`] - Simplified schema representation for diffing
//! - [`SimpleModel`] - Model definition for diffing
//! - [`SimpleField`] - Field definition for diffing
//! - [`SimpleConstraint`] - Constraint definition for diffing
//!
//! ## Migration Types
//!
//! - [`Migration`] - A migration with up and down operations
//! - [`MigrationChange`] - Individual schema change operation
//! - [`MigrationStatus`] - Applied, pending, or rolled back
//!
//! # Supported Changes
//!
//! ## Model Changes
//!
//! - **Add Model** - Create a new model (table)
//! - **Remove Model** - Drop an existing model
//! - **Rename Model** - Rename a model
//!
//! ## Field Changes
//!
//! - **Add Field** - Add a new field to a model
//! - **Remove Field** - Remove a field from a model
//! - **Rename Field** - Rename a field
//! - **Change Type** - Change field data type
//! - **Change Nullable** - Make field nullable or non-nullable
//!
//! ## Constraint Changes
//!
//! - **Add Constraint** - Add a constraint to a field
//! - **Remove Constraint** - Remove a constraint from a field
//!
//! # Migration Files
//!
//! Migrations are stored as JSON files:
//!
//! ```json
//! {
//!   "name": "20240101_add_user_email",
//!   "timestamp": 1704067200,
//!   "up": [
//!     {
//!       "type": "AddField",
//!       "model": "User",
//!       "field": "email",
//!       "field_type": "string"
//!     }
//!   ],
//!   "down": [
//!     {
//!       "type": "RemoveField",
//!       "model": "User",
//!       "field": "email"
//!     }
//!   ]
//! }
//! ```
//!
//! # Safety
//!
//! The migration system includes safety features:
//!
//! - **Reversibility** - Every migration has up and down operations
//! - **Tracking** - Applied migrations are tracked to prevent duplicate application
//! - **Validation** - Migrations are validated before execution
//! - **Atomic Operations** - Changes are applied atomically where possible
//!
//! # Related Crates
//!
//! - [`forgedb-parser`](../forgedb_parser) - Parses schema files
//! - [`forgedb-storage`](../forgedb_storage) - Applies migration changes to storage
//! - [`forgedb-validation`](../forgedb_validation) - Validates schema changes
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [SPRINT10_MIGRATIONS.md](../../archive/sprint-summaries/SPRINT10_MIGRATIONS.md) - Migration system implementation

pub mod diff;
mod executor;
mod generator;
mod tracker;
mod types;

pub use diff::{SchemaDiffer, SimpleSchema, SimpleModel, SimpleField, SimpleConstraint};
pub use executor::MigrationExecutor;
pub use generator::MigrationGenerator;
pub use tracker::MigrationTracker;
pub use types::*;

#[cfg(test)]
mod tests;
