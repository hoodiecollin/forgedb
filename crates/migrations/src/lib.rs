//! ForgeDB Migrations
//!
//! Database schema migration generation, tracking, and execution for ForgeDB.
//!
//! # Overview
//!
//! This crate provides the **dev-time** half of ForgeDB's schema-migration system
//! (#74): it records how a schema evolves and classifies each hop, so the codegen
//! can emit a per-range offline **transformer bin** that rewrites data-at-rest.
//! It handles:
//!
//! - **Schema diffing** - Compare old and new schemas to detect changes
//! - **Migration generation** - Create versioned migration records from differences
//! - **Lineage** - The serial `from -> to` version chain the transformer replays
//! - **Migration tracking** - Track which migrations have been applied
//!
//! Note: this crate does **not** apply changes to data at runtime.  There is no
//! `MigrationExecutor`; the actual byte rewrite is done by generated code — the
//! transformer bin `forgedb-codegen`'s `TransformGenerator` emits (Phase 3) — so
//! the schema is never a runtime input to a generic engine (the generator-identity
//! red line).
//!
//! # Architecture
//!
//! The migration system consists of these components:
//!
//! 1. **SchemaDiffer** - Compares schemas and identifies changes
//! 2. **MigrationGenerator** - Generates versioned migration records from diffs
//! 3. **MigrationLineage** - Orders the serial version chain + expands a range
//! 4. **MigrationTracker** - Tracks applied migrations
//!
//! ## Migration Flow
//!
//! ```text
//! Schema v1 → Schema v2
//!     ↓
//! SchemaDiffer (detect changes)
//!     ↓
//! Migration { changes, from_version, to_version }  ← recorded + classified
//!     ↓
//! TransformGenerator (codegen) → offline transformer bin
//!     ↓
//! `forgedb migrate build` + `migrate run --from 1 --to 2`  → data rewritten
//! ```
//!
//! # Examples
//!
//! ## Detecting Schema Changes
//!
//! ```rust
//! use forgedb_migrations::{
//!     SchemaDiffer, SimpleSchema, SimpleModel, SimpleField, SimpleConstraint, SimpleType,
//! };
//!
//! // Create schemas directly (no file loading at the diff level)
//! let old_schema = SimpleSchema {
//!     models: vec![
//!         SimpleModel {
//!             name: "User".to_string(),
//!             fields: vec![
//!                 SimpleField {
//!                     name: "id".to_string(),
//!                     ty: SimpleType::Uuid,
//!                     nullable: false,
//!                     unique: false,
//!                     indexed: false,
//!                     index_type: "hash".to_string(),
//!                     constraints: vec![],
//!                     depends_on: vec![],
//!                 },
//!             ],
//!             composite_indexes: vec![],
//!         },
//!     ],
//!     enums: vec![],
//!     structs: vec![],
//! };
//!
//! let new_schema = SimpleSchema {
//!     models: vec![
//!         SimpleModel {
//!             name: "User".to_string(),
//!             fields: vec![
//!                 SimpleField {
//!                     name: "id".to_string(),
//!                     ty: SimpleType::Uuid,
//!                     nullable: false,
//!                     unique: false,
//!                     indexed: false,
//!                     index_type: "hash".to_string(),
//!                     constraints: vec![],
//!                     depends_on: vec![],
//!                 },
//!                 SimpleField {
//!                     name: "email".to_string(),
//!                     ty: SimpleType::Str,
//!                     nullable: false,
//!                     unique: false,
//!                     indexed: false,
//!                     index_type: "hash".to_string(),
//!                     constraints: vec![],
//!                     depends_on: vec![],
//!                 },
//!             ],
//!             composite_indexes: vec![],
//!         },
//!     ],
//!     enums: vec![],
//!     structs: vec![],
//! };
//!
//! // SchemaDiffer is a unit struct; call diff() as a static method
//! let changes = SchemaDiffer::diff(&old_schema, &new_schema);
//! println!("Detected {} changes", changes.len());
//! assert_eq!(changes.len(), 1);
//! ```
//!
//! ## Applying Migrations
//!
//! ```rust,no_run
//! use forgedb_migrations::{MigrationTracker, Migration, SchemaChange};
//!
//! // MigrationTracker::new returns Result<Self, String>
//! let mut tracker = MigrationTracker::new("./migrations").expect("tracker init");
//!
//! let migration = Migration::new("add email".to_string(), vec![
//!     SchemaChange::AddModel { model_name: "Post".to_string() },
//! ]);
//!
//! // pending_migrations takes a slice of migration id strings
//! let all_ids = vec!["20240101_add_email".to_string()];
//! let pending = tracker.pending_migrations(&all_ids);
//!
//! // mark_applied requires (id, checksum)
//! // tracker.mark_applied("20240101_add_email".to_string(), "abc".to_string()).unwrap();
//! // tracker.last_migration() returns Option<&MigrationRecord>
//! let _ = tracker.last_migration();
//! // tracker.mark_rolled_back() (no args) removes last record
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`SchemaDiffer`] - Schema comparison and change detection
//! - [`MigrationGenerator`] - Migration file generation
//! - [`MigrationLineage`] - Serial version lineage for the offline transformer bin
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
//! - [`Migration`] - A recorded, versioned (`from -> to`) set of schema changes
//! - [`SchemaChange`] - An individual schema change operation
//! - [`HopBodyClass`] - Whether a hop's new-row body is `Auto` or `Authored`
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
//! Each migration is a JSON record under `migrations/`, carrying its serial
//! version span (`from_version -> to_version`) and its detected changes:
//!
//! ```json
//! {
//!   "id": "20240101120000",
//!   "description": "add user email",
//!   "changes": [
//!     { "AddField": { "model_name": "User", "field_name": "email",
//!                     "field_type": "string", "nullable": true,
//!                     "default_json": null } }
//!   ],
//!   "from_version": 1,
//!   "to_version": 2
//! }
//! ```
//!
//! The full `.forge` source of each version is snapshotted alongside under
//! `migrations/schemas/v{n}.forge`, and any `Authored` hop's frozen transform
//! lives at `migrations/{id}/transform.rs`.
//!
//! # Safety
//!
//! The migration system includes safety features:
//!
//! - **Version interlock** - A regenerated app refuses a data dir at the wrong
//!   schema serial rather than mis-decoding it (fail-fast, never self-heals)
//! - **Rollback by retention** - The transformer writes a fresh destination dir
//!   and leaves the source untouched, so the original is the rollback
//! - **Tracking** - Applied migrations are tracked to prevent duplicate application
//! - **Atomic publish** - A multi-hop range materializes via temp dirs and a
//!   single final rename (all-or-nothing)
//!
//! # Related Crates
//!
//! - [`forgedb-parser`](../forgedb_parser) - Parses schema files
//! - [`forgedb-codegen`](../forgedb_codegen) - Emits the offline transformer bin
//! - [`forgedb-validation`](../forgedb_validation) - Validates schema changes
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [SPRINT10_MIGRATIONS.md](../../archive/sprint-summaries/SPRINT10_MIGRATIONS.md) - Migration system implementation

pub mod diff;
mod generator;
pub mod lineage;
mod tracker;
mod types;

pub use diff::{
    SchemaDiffer, SimpleConstraint, SimpleEnum, SimpleField, SimpleModel, SimpleSchema,
    SimpleStruct, SimpleType,
};
pub use generator::MigrationGenerator;
pub use lineage::{
    BASELINE_SCHEMA_VERSION, MigrationLineage, Unanswered, authored_body_path,
    current_schema_version, escape_body_path, hop_answer_status, load_versioned_schema,
    migration_body_dir, save_versioned_schema, scaffold_authored_body, versioned_schema_dir,
    versioned_schema_path,
};
pub use tracker::MigrationTracker;
pub use types::*;

#[cfg(test)]
mod tests;
