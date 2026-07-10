// Query planning and cost-based optimization
//
// This module implements:
// - Cost-based index selection
// - Join order optimization
// - Predicate pushdown

use std::collections::{BTreeSet, HashMap, HashSet};

/// Comparison operator in a structured predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// One side of a predicate: a (possibly table-qualified) column reference, or a
/// literal value carried verbatim (numbers, quoted strings, `true`/`false`/`null`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// `table.column` (qualified) or bare `column` (`table == None`).
    Column {
        table: Option<String>,
        column: String,
    },
    /// An opaque literal, kept as its source text.
    Literal(String),
}

impl Operand {
    /// Classify a trimmed operand token as a literal or a column reference.
    fn parse(token: &str) -> Operand {
        let t = token.trim();
        let is_literal = match t.chars().next() {
            Some(c) if c.is_ascii_digit() => true,
            Some('-') | Some('+') | Some('.') | Some('\'') | Some('"') => true,
            _ => matches!(t, "true" | "false" | "null" | "NULL"),
        };
        if is_literal {
            Operand::Literal(t.to_string())
        } else if let Some((tbl, col)) = t.split_once('.') {
            Operand::Column {
                table: Some(tbl.trim().to_string()),
                column: col.trim().to_string(),
            }
        } else {
            Operand::Column {
                table: None,
                column: t.to_string(),
            }
        }
    }
}

/// A structured predicate `<left> <op> <right>` parsed from a plan predicate
/// string. This is the predicate IR that lets the planner attribute a predicate
/// to a specific join side (see [`Predicate::tables_referenced`]).
///
/// The original source text is retained (`raw`) so a predicate can be re-emitted
/// verbatim into a `Filter` node — the plan continues to speak strings, the IR is
/// only an analysis lens over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    pub left: Operand,
    pub op: PredicateOp,
    pub right: Operand,
    pub raw: String,
}

impl Predicate {
    /// Parse a predicate string of the form `<operand> <op> <operand>`.
    ///
    /// Returns `None` when no comparison operator is present — the caller then
    /// treats the predicate as unattributable and keeps it at the join output.
    pub fn parse(raw: &str) -> Option<Predicate> {
        // Ordered longest-first so `>=` is matched before `>`, etc.
        const OPS: &[(&str, PredicateOp)] = &[
            (">=", PredicateOp::Gte),
            ("<=", PredicateOp::Lte),
            ("!=", PredicateOp::Ne),
            ("<>", PredicateOp::Ne),
            ("=", PredicateOp::Eq),
            (">", PredicateOp::Gt),
            ("<", PredicateOp::Lt),
        ];
        // Scan left-to-right for the earliest operator; at each position the
        // longest matching token wins (OPS is ordered to make that true).
        for i in 0..raw.len() {
            if !raw.is_char_boundary(i) {
                continue;
            }
            for (tok, op) in OPS {
                if raw[i..].starts_with(tok) {
                    let left = Operand::parse(&raw[..i]);
                    let right = Operand::parse(&raw[i + tok.len()..]);
                    return Some(Predicate {
                        left,
                        op: *op,
                        right,
                        raw: raw.trim().to_string(),
                    });
                }
            }
        }
        None
    }

    /// The set of table names this predicate references through its qualified
    /// column operands. Bare (unqualified) columns and literals contribute
    /// nothing — so a predicate with no `table.column` operand refers to no
    /// table and cannot be pushed to a side.
    pub fn tables_referenced(&self) -> BTreeSet<String> {
        let mut tables = BTreeSet::new();
        for operand in [&self.left, &self.right] {
            if let Operand::Column {
                table: Some(t), ..
            } = operand
            {
                tables.insert(t.clone());
            }
        }
        tables
    }
}

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
    /// Filter operation wrapping the sub-plan it applies to.
    Filter {
        /// The operation whose output rows this filter is applied to.
        input: Box<QueryPlanOp>,
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
                    // Index scan cost = index lookup + fetch.
                    // Floor log2 to f64::EPSILON so zero/tiny selectivity never yields
                    // -inf or NaN, which would cause total_cmp to produce surprising
                    // orderings and the previous partial_cmp().unwrap() to panic.
                    let index_lookup_cost = (selectivity * index.cardinality as f64)
                        .log2()
                        .max(f64::EPSILON);
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

        // Sort by cost (lowest first). Use total_cmp so NaN values (which can't
        // arise now but are guarded against defensively) sort deterministically
        // rather than panicking.
        estimates.sort_by(|a, b| a.cost.total_cmp(&b.cost));

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

    /// Apply predicate pushdown optimization.
    ///
    /// Moves filter predicates as close to the data source as possible. Over a
    /// join, each predicate is attributed to a side by the tables its columns
    /// reference (via the predicate IR): a predicate touching only the left
    /// side's tables is pushed into the left sub-plan, only the right side's into
    /// the right, and a genuinely cross-side predicate (a join condition, an
    /// unqualified column, or an unparseable string) is preserved as a `Filter`
    /// wrapping the join output. No predicate is ever dropped.
    pub fn pushdown_predicates(
        &self,
        plan: QueryPlanOp,
        predicates: Vec<String>,
    ) -> QueryPlanOp {
        match plan {
            QueryPlanOp::Join { left, right, join_type, estimated_rows } => {
                // Collect the tables reachable through each side so a predicate can
                // be matched to the side that can actually evaluate it.
                let mut left_tables = HashSet::new();
                collect_tables(&left, &mut left_tables);
                let mut right_tables = HashSet::new();
                collect_tables(&right, &mut right_tables);

                let (left_preds, right_preds, remaining) =
                    self.partition_predicates_for_join(&predicates, &left_tables, &right_tables);

                let optimized_left = if left_preds.is_empty() {
                    left
                } else {
                    Box::new(self.pushdown_predicates(*left, left_preds))
                };

                let optimized_right = if right_preds.is_empty() {
                    right
                } else {
                    Box::new(self.pushdown_predicates(*right, right_preds))
                };

                let join = QueryPlanOp::Join {
                    left: optimized_left,
                    right: optimized_right,
                    join_type,
                    estimated_rows,
                };

                // Wrap the (already pushed-down) join in a Filter when predicates
                // remain unattributed, ensuring no predicate is silently dropped.
                if remaining.is_empty() {
                    join
                } else {
                    QueryPlanOp::Filter {
                        input: Box::new(join),
                        predicate: remaining.join(" AND "),
                        input_rows: estimated_rows,
                        selectivity: 0.1, // Conservative estimate
                    }
                }
            }
            QueryPlanOp::TableScan { table, row_count } => {
                // Apply predicates directly on top of the table scan.
                if predicates.is_empty() {
                    QueryPlanOp::TableScan { table, row_count }
                } else {
                    let selectivity = 0.1; // Estimate, could be improved
                    QueryPlanOp::Filter {
                        input: Box::new(QueryPlanOp::TableScan { table, row_count }),
                        predicate: predicates.join(" AND "),
                        input_rows: row_count,
                        selectivity,
                    }
                }
            }
            other => other,
        }
    }

    /// Partition predicates for the left/right sides of a join.
    ///
    /// Each predicate is parsed into the predicate IR and attributed by the
    /// tables its qualified columns reference:
    /// - references only tables on the left side → left,
    /// - references only tables on the right side → right,
    /// - references both sides, an unknown table, only unqualified columns, or
    ///   fails to parse → remaining (kept at the join output).
    ///
    /// Returns `(left, right, remaining)` as the original predicate strings so
    /// they round-trip verbatim into the plan.
    fn partition_predicates_for_join(
        &self,
        predicates: &[String],
        left_tables: &HashSet<String>,
        right_tables: &HashSet<String>,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut remaining = Vec::new();

        for p in predicates {
            let refs = match Predicate::parse(p) {
                Some(pred) => pred.tables_referenced(),
                None => {
                    remaining.push(p.clone());
                    continue;
                }
            };

            if !refs.is_empty() && refs.iter().all(|t| left_tables.contains(t)) {
                left.push(p.clone());
            } else if !refs.is_empty() && refs.iter().all(|t| right_tables.contains(t)) {
                right.push(p.clone());
            } else {
                remaining.push(p.clone());
            }
        }

        (left, right, remaining)
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

        // Add filters, wrapping the scan they apply to.
        if !filters.is_empty() {
            let input = operations.last().unwrap().clone();
            let filter_op = QueryPlanOp::Filter {
                input: Box::new(input),
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

/// Collect every table name reachable through a plan sub-tree, so a join can
/// know which tables each of its sides can evaluate predicates against.
fn collect_tables(op: &QueryPlanOp, out: &mut HashSet<String>) {
    match op {
        QueryPlanOp::TableScan { table, .. }
        | QueryPlanOp::IndexScan { table, .. }
        | QueryPlanOp::IndexRangeScan { table, .. } => {
            out.insert(table.clone());
        }
        QueryPlanOp::Filter { input, .. } => collect_tables(input, out),
        QueryPlanOp::Join { left, right, .. } => {
            collect_tables(left, out);
            collect_tables(right, out);
        }
        QueryPlanOp::Limit { input, .. } => collect_tables(input, out),
    }
}

