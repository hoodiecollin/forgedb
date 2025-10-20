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
