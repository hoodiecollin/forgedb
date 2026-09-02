pub mod diff;
mod generator;
pub mod lineage;
mod tracker;
mod types;

pub use diff::{
    DiffResult, RenameProposal, SchemaDiffer, SimpleConstraint, SimpleEnum, SimpleField,
    SimpleModel, SimpleSchema, SimpleStruct, SimpleType,
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
