# ForgeDB Query Optimization

High-performance query optimization crate for ForgeDB, featuring SIMD-accelerated scanning, cost-based planning, and comprehensive statistics tracking.

## Features

### 🚀 SIMD Columnar Scanning
- AVX2-optimized scanning for `u64` values (4 values per instruction)
- Batch processing (1024 rows per batch)
- Early termination for LIMIT queries
- Support for `u64`, `i64`, and `f64` data types

### 📊 Query Planning
- Cost-based index selection
- Join order optimization (smallest-first strategy)
- Predicate pushdown optimization
- Table and index statistics tracking

### 📈 Index Statistics
- Lookup and range scan counting
- Cardinality and selectivity tracking
- Index size monitoring
- Staleness detection

## Performance

**Benchmark Results (1M rows):**
- Equality scan: **~848 µs** (< 1ms)
- Range scan: **~975 µs** (< 1ms)
- GT scan: **~927 µs** (< 1ms)
- Throughput: **1.18 Gelem/s**

**All targets exceeded:** ✅ Sprint goal was < 100ms for 1M rows!

## Quick Start

```rust
use forgedb_query_optimization::{ColumnScan, ScanFilter};

// Scan 1M rows with equality filter
let data: Vec<u64> = (0..1_000_000).collect();
let result = ColumnScan::scan_u64(&data, ScanFilter::Eq(42), None);

println!("Found: {}", result.matching_rows.len());
println!("Scanned: {} rows", result.rows_scanned);
```

## Running Tests

```bash
cargo test -p forgedb-query-optimization
```

## Running Benchmarks

```bash
cargo bench -p forgedb-query-optimization
```

## Documentation

See [`archive/sprint-summaries/SPRINT14_QUERY_OPTIMIZATION.md`](../../archive/sprint-summaries/SPRINT14_QUERY_OPTIMIZATION.md) for complete documentation.

## License

Part of the ForgeDB project.
