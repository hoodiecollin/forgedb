// Sprint 14: Query Optimization
//
// This crate provides:
// - Columnar scanning with SIMD operations
// - Batch processing for efficient filtering
// - Query planning and cost estimation
// - Index statistics tracking

pub mod scan;
pub mod planner;
pub mod statistics;

pub use scan::{ColumnScan, ScanFilter, ScanResult};
pub use planner::{QueryPlan, QueryPlanner, CostEstimate};
pub use statistics::{IndexStats, IndexStatistics};
