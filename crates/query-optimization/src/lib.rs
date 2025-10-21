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
//! ```rust,no_run
//! use forgedb_query_optimization::{ColumnScan, ScanFilter};
//!
//! // Create a column scanner
//! let scanner = ColumnScan::new("./data/User");
//!
//! // Define filter predicate
//! let filter = ScanFilter::GreaterThan {
//!     column: "age".to_string(),
//!     value: 18,
//! };
//!
//! // Execute scan
//! let results = scanner.scan(&filter)?;
//! println!("Found {} matching rows", results.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Query Planning
//!
//! ```rust,no_run
//! use forgedb_query_optimization::{QueryPlanner, QueryPlan};
//!
//! let planner = QueryPlanner::new("./data");
//!
//! // Analyze query and generate plan
//! let plan = planner.plan_query(
//!     "User",
//!     vec!["age > 18", "email LIKE '%@example.com'"]
//! )?;
//!
//! // Inspect estimated cost
//! println!("Estimated cost: {}", plan.estimated_cost.total_cost);
//! println!("Estimated rows: {}", plan.estimated_cost.estimated_rows);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Statistics Tracking
//!
//! ```rust,no_run
//! use forgedb_query_optimization::IndexStatistics;
//!
//! let mut stats = IndexStatistics::new("./data");
//!
//! // Collect statistics for a model
//! stats.collect_model_stats("User")?;
//!
//! // Query statistics
//! let age_stats = stats.get_column_stats("User", "age")?;
//! println!("Min: {}, Max: {}, Distinct: {}",
//!     age_stats.min_value,
//!     age_stats.max_value,
//!     age_stats.distinct_values
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
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
//! - [`forgedb-fulltext`](../forgedb_fulltext) - Full-text search capabilities
//! - [`forgedb-crud-api`](../forgedb_crud_api) - High-level CRUD operations
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [SPRINT14_SUMMARY.md](../../archive/sprint-summaries/SPRINT14_SUMMARY.md) - Original implementation

pub mod scan;
pub mod planner;
pub mod statistics;

pub use scan::{ColumnScan, ScanFilter, ScanResult};
pub use planner::{QueryPlan, QueryPlanner, CostEstimate};
pub use statistics::{IndexStats, IndexStatistics};
