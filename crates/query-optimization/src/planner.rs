// Query planning and cost-based optimization
//
// This module implements:
// - Cost-based index selection
// - Join order optimization
// - Predicate pushdown

use std::collections::HashMap;

/// Cost estimate for a query operation
#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimate {
    /// Estimated number of rows to scan
    pub rows_scanned: usize,
    /// Estimated number of rows returned
    pub rows_returned: usize,
    /// Cost in arbitrary units (lower is better)
    pub cost: f64,
    /// Whether an index can be used
    pub uses_index: bool,
    /// Index name if applicable
    pub index_name: Option<String>,
}

/// Query plan operation
#[derive(Debug, Clone)]
pub enum QueryPlanOp {
    /// Full table scan
    TableScan {
        table: String,
        row_count: usize,
    },
    /// Index lookup
    IndexScan {
        table: String,
        index: String,
        selectivity: f64,
    },
    /// Range scan on index
    IndexRangeScan {
        table: String,
        index: String,
        selectivity: f64,
    },
    /// Filter operation
    Filter {
        predicate: String,
        input_rows: usize,
        selectivity: f64,
    },
    /// Join operation
    Join {
        left: Box<QueryPlanOp>,
        right: Box<QueryPlanOp>,
        join_type: JoinType,
        estimated_rows: usize,
    },
    /// Limit operation
    Limit {
        input: Box<QueryPlanOp>,
        limit: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
}

/// Query plan with cost estimation
#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub operations: Vec<QueryPlanOp>,
    pub total_cost: f64,
}

/// Statistics for a table
#[derive(Debug, Clone)]
pub struct TableStats {
    pub name: String,
    pub row_count: usize,
    pub avg_row_size: usize,
}

/// Statistics for an index
#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub cardinality: usize,
}

/// Query planner with cost-based optimization
pub struct QueryPlanner {
    table_stats: HashMap<String, TableStats>,
    index_info: HashMap<String, Vec<IndexInfo>>,
}

impl QueryPlanner {
    pub fn new() -> Self {
        QueryPlanner {
            table_stats: HashMap::new(),
            index_info: HashMap::new(),
        }
    }

    /// Register table statistics
    pub fn register_table(&mut self, stats: TableStats) {
        self.table_stats.insert(stats.name.clone(), stats);
    }

    /// Register index information
    pub fn register_index(&mut self, index: IndexInfo) {
        self.index_info
            .entry(index.table.clone())
            .or_insert_with(Vec::new)
            .push(index);
    }

    /// Estimate cost of different scan strategies
    pub fn estimate_scan_cost(
        &self,
        table: &str,
        column: &str,
        selectivity: f64,
    ) -> Vec<CostEstimate> {
        let mut estimates = Vec::new();

        // Get table stats
        let table_stats = match self.table_stats.get(table) {
            Some(stats) => stats,
            None => return estimates,
        };

        // 1. Full table scan cost
        let table_scan_cost = table_stats.row_count as f64;
        estimates.push(CostEstimate {
            rows_scanned: table_stats.row_count,
            rows_returned: (table_stats.row_count as f64 * selectivity) as usize,
            cost: table_scan_cost,
            uses_index: false,
            index_name: None,
        });

        // 2. Index scan costs (if applicable)
        if let Some(indexes) = self.index_info.get(table) {
            for index in indexes {
                if index.columns.contains(&column.to_string()) {
                    // Index scan cost = index lookup + fetch
                    let index_lookup_cost = (selectivity * index.cardinality as f64).log2();
                    let fetch_cost = table_stats.row_count as f64 * selectivity;
                    let total_cost = index_lookup_cost + fetch_cost;

                    estimates.push(CostEstimate {
                        rows_scanned: (table_stats.row_count as f64 * selectivity) as usize,
                        rows_returned: (table_stats.row_count as f64 * selectivity) as usize,
                        cost: total_cost,
                        uses_index: true,
                        index_name: Some(index.name.clone()),
                    });
                }
            }
        }

        // Sort by cost (lowest first)
        estimates.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());

        estimates
    }

    /// Choose best index for a query
    pub fn choose_index(
        &self,
        table: &str,
        column: &str,
        selectivity: f64,
    ) -> Option<String> {
        let estimates = self.estimate_scan_cost(table, column, selectivity);

        // Return the best (lowest cost) option that uses an index
        estimates
            .into_iter()
            .find(|e| e.uses_index)
            .and_then(|e| e.index_name)
    }

    /// Optimize join order based on table sizes
    pub fn optimize_join_order(&self, tables: &[String]) -> Vec<String> {
        let mut table_sizes: Vec<_> = tables
            .iter()
            .map(|t| {
                let size = self
                    .table_stats
                    .get(t)
                    .map(|s| s.row_count)
                    .unwrap_or(usize::MAX);
                (t.clone(), size)
            })
            .collect();

        // Sort by size (smallest first) for better join performance
        table_sizes.sort_by_key(|(_name, size)| *size);

        table_sizes.into_iter().map(|(name, _)| name).collect()
    }

    /// Apply predicate pushdown optimization
    ///
    /// Moves filter predicates as close to the data source as possible
    pub fn pushdown_predicates(
        &self,
        plan: QueryPlanOp,
        predicates: Vec<String>,
    ) -> QueryPlanOp {
        match plan {
            QueryPlanOp::Join { left, right, join_type, estimated_rows } => {
                // Push predicates down to join inputs if applicable
                let (left_preds, right_preds) = self.partition_predicates_for_join(&predicates);

                let optimized_left = if !left_preds.is_empty() {
                    Box::new(self.pushdown_predicates(*left, left_preds))
                } else {
                    left
                };

                let optimized_right = if !right_preds.is_empty() {
                    Box::new(self.pushdown_predicates(*right, right_preds))
                } else {
                    right
                };

                QueryPlanOp::Join {
                    left: optimized_left,
                    right: optimized_right,
                    join_type,
                    estimated_rows,
                }
            }
            QueryPlanOp::TableScan { table, row_count } => {
                // Apply predicates directly to table scan
                if predicates.is_empty() {
                    QueryPlanOp::TableScan { table, row_count }
                } else {
                    // Create filter operation on top of scan
                    let selectivity = 0.1; // Estimate, could be improved
                    QueryPlanOp::Filter {
                        predicate: predicates.join(" AND "),
                        input_rows: row_count,
                        selectivity,
                    }
                }
            }
            other => other,
        }
    }

    /// Partition predicates for left/right sides of join
    fn partition_predicates_for_join(&self, predicates: &[String]) -> (Vec<String>, Vec<String>) {
        // Simple implementation - in reality would parse predicates
        // and determine which side of join they apply to
        let left = Vec::new();
        let right = Vec::new();
        (left, right)
    }

    /// Create a complete query plan with cost estimation
    pub fn create_plan(&self, table: &str, filters: Vec<String>, limit: Option<usize>) -> QueryPlan {
        let table_stats = self.table_stats.get(table);
        let row_count = table_stats.map(|s| s.row_count).unwrap_or(0);

        let mut operations = Vec::new();
        let mut current_cost = 0.0;

        // Start with scan operation
        let scan_op = QueryPlanOp::TableScan {
            table: table.to_string(),
            row_count,
        };
        current_cost += row_count as f64;
        operations.push(scan_op);

        // Add filters
        if !filters.is_empty() {
            let filter_op = QueryPlanOp::Filter {
                predicate: filters.join(" AND "),
                input_rows: row_count,
                selectivity: 0.1, // Estimate
            };
            current_cost += row_count as f64 * 0.1;
            operations.push(filter_op);
        }

        // Add limit
        if let Some(limit_val) = limit {
            let filter_op = QueryPlanOp::Limit {
                input: Box::new(operations.last().unwrap().clone()),
                limit: limit_val,
            };
            current_cost *= 0.5; // Limit reduces cost
            operations.push(filter_op);
        }

        QueryPlan {
            operations,
            total_cost: current_cost,
        }
    }
}

impl Default for QueryPlanner {
    fn default() -> Self {
        Self::new()
    }
}

