//! ForgeDB Query Optimization
//!
//! High-performance query execution with columnar scanning, SIMD operations, and cost-based
//! query planning for ForgeDB's storage engine.
//!
//! # Overview
//!
//! This crate provides query optimization capabilities for ForgeDB, leveraging columnar
//! storage layout for efficient data scanning and filtering. Key features include:
//!
//! - **Columnar scanning** - Efficient column-oriented data access
//! - **SIMD operations** - Vectorized operations for numeric filtering
//! - **Batch processing** - Process multiple rows efficiently
//! - **Query planning** - Cost-based query optimization
//! - **Statistics tracking** - Index and column statistics for query planning
//!
//! # Architecture
//!
//! ## Query Execution Pipeline
//!
//! 1. **Query Planning** - Analyze query and generate execution plan
//! 2. **Cost Estimation** - Estimate query cost using statistics
//! 3. **Columnar Scanning** - Scan columns efficiently in batches
//! 4. **SIMD Filtering** - Apply vectorized filters on numeric data
//! 5. **Result Collection** - Gather matching rows
//!
//! ## Components
//!
//! - **ColumnScan**: Low-level columnar scanning with SIMD optimizations
//! - **QueryPlanner**: Cost-based query planning and optimization
//! - **IndexStatistics**: Track and maintain column statistics
//!
//! # Examples
//!
//! ## Columnar Scanning
//!
//! ```rust
//! use forgedb_query_optimization::{ColumnScan, ScanFilter};
//!
//! // Scan a u64 column for values greater than 18
//! let column_data: Vec<u64> = vec![5, 10, 20, 30, 15, 25];
//! let filter = ScanFilter::Gt(18u64);
//! let result = ColumnScan::scan_u64(&column_data, filter, None);
//! println!("Found {} matching rows", result.matching_rows.len());
//! assert_eq!(result.matching_rows.len(), 3); // indices 2 (20), 3 (30), 5 (25)
//! ```
//!
//! ## Query Planning
//!
//! ```rust
//! use forgedb_query_optimization::{QueryPlanner, planner::TableStats};
//!
//! let mut planner = QueryPlanner::new();
//!
//! // Register table statistics
//! planner.register_table(TableStats {
//!     name: "User".to_string(),
//!     row_count: 10_000,
//!     avg_row_size: 128,
//! });
//!
//! // Create a query plan with filters and a limit
//! let plan = planner.create_plan(
//!     "User",
//!     vec!["age > 18".to_string()],
//!     Some(100),
//! );
//! println!("Estimated cost: {}", plan.total_cost);
//! assert!(plan.total_cost > 0.0);
//! ```
//!
//! ## Statistics Tracking
//!
//! ```rust
//! use forgedb_query_optimization::IndexStatistics;
//!
//! let mut stats = IndexStatistics::new();
//!
//! // Register an index
//! stats.register_index(
//!     "age_idx".to_string(),
//!     "User".to_string(),
//!     vec!["age".to_string()],
//!     false,
//! );
//!
//! // Update and query statistics
//! stats.update_index_stats("age_idx", 10_000, 500, 40_960);
//! if let Some(age_stats) = stats.get_stats("age_idx") {
//!     println!("Cardinality: {}", age_stats.cardinality);
//!     assert_eq!(age_stats.cardinality, 500);
//! }
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`ColumnScan`] - Columnar scanning with SIMD optimizations
//! - [`QueryPlanner`] - Cost-based query planning
//! - [`IndexStatistics`] - Column statistics tracking
//!
//! ## Supporting Types
//!
//! - [`ScanFilter`] - Filter predicates for columnar scans
//! - [`ScanResult`] - Results from columnar scans
//! - [`QueryPlan`] - Query execution plan
//! - [`CostEstimate`] - Estimated query execution cost
//! - [`IndexStats`] - Statistics for a single column
//!
//! # Performance Characteristics
//!
//! ## SIMD Operations
//!
//! The crate uses SIMD (Single Instruction, Multiple Data) operations for:
//! - Numeric comparisons (>, <, ==, etc.)
//! - Range filtering
//! - Batch processing of rows
//!
//! **Performance benefits**:
//! - 4-8x faster numeric filtering
//! - Reduced CPU cycles per row
//! - Better cache utilization
//!
//! ## Columnar Scanning
//!
//! Columnar layout enables:
//! - Sequential memory access patterns
//! - Selective column reading (only read needed columns)
//! - Better compression ratios
//! - Cache-friendly data access
//!
//! # Related Crates
//!
//! - [`forgedb-storage`](../forgedb_storage) - Columnar storage engine
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation

pub mod scan;
pub mod planner;
pub mod statistics;

pub use scan::{ColumnScan, ScanFilter, ScanResult};
pub use planner::{QueryPlan, QueryPlanner, CostEstimate};
pub use statistics::{IndexStats, IndexStatistics};
