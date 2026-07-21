// Benchmark for columnar scanning performance
//
// Sprint 14 targets:
// - Scan 1M rows: < 100ms
// - Index lookup: < 1μs
// - Join 10k → 100k: < 50ms

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use forgedb_query_optimization::{ColumnScan, ScanFilter};

fn bench_scan_1m_rows(c: &mut Criterion) {
    let mut group = c.benchmark_group("scan_1m_rows");

    // Create 1M rows of test data
    let data: Vec<u64> = (0..1_000_000).collect();

    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10); // Reduce samples for long-running benchmarks

    // Benchmark: Scan 1M rows with equality filter
    group.bench_function("eq_filter", |b| {
        b.iter(|| {
            let result = ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Eq(500_000)),
                None,
            );
            black_box(result)
        })
    });

    // Benchmark: Scan 1M rows with range filter
    group.bench_function("range_filter", |b| {
        b.iter(|| {
            let result = ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Range(100_000, 200_000)),
                None,
            );
            black_box(result)
        })
    });

    // Benchmark: Scan 1M rows with GT filter
    group.bench_function("gt_filter", |b| {
        b.iter(|| {
            let result = ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Gt(950_000)),
                None,
            );
            black_box(result)
        })
    });

    // Benchmark: Early termination with LIMIT
    group.bench_function("early_termination_limit_10", |b| {
        b.iter(|| {
            let result = ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Gte(0)),
                Some(10),
            );
            black_box(result)
        })
    });

    group.finish();
}

fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");

    // Test different data sizes
    for size in [1_000, 10_000, 100_000, 1_000_000].iter() {
        let data: Vec<u64> = (0..*size).collect();

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, _| {
                b.iter(|| {
                    let result = ColumnScan::scan_u64(
                        black_box(&data),
                        black_box(ScanFilter::Gt(*size / 2)),
                        None,
                    );
                    black_box(result)
                })
            },
        );
    }

    group.finish();
}

fn bench_different_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("filter_types");

    let data: Vec<u64> = (0..100_000).collect();
    group.throughput(Throughput::Elements(100_000));

    group.bench_function("eq", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Eq(50_000)),
                None,
            )
        })
    });

    group.bench_function("ne", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Ne(50_000)),
                None,
            )
        })
    });

    group.bench_function("gt", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Gt(50_000)),
                None,
            )
        })
    });

    group.bench_function("gte", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Gte(50_000)),
                None,
            )
        })
    });

    group.bench_function("lt", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Lt(50_000)),
                None,
            )
        })
    });

    group.bench_function("lte", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Lte(50_000)),
                None,
            )
        })
    });

    group.bench_function("range", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Range(25_000, 75_000)),
                None,
            )
        })
    });

    group.finish();
}

fn bench_simd_vs_scalar(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_comparison");

    let data: Vec<u64> = (0..1_000_000).collect();
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10);

    // SIMD-optimized scan (will use AVX2 if available)
    group.bench_function("simd_eq", |b| {
        b.iter(|| {
            ColumnScan::scan_u64(
                black_box(&data),
                black_box(ScanFilter::Eq(500_000)),
                None,
            )
        })
    });

    // Note: For scalar comparison, we'd need to expose the scalar method
    // or disable SIMD features during compilation

    group.finish();
}

fn bench_i64_scans(c: &mut Criterion) {
    let mut group = c.benchmark_group("i64_scans");

    // Test with negative numbers
    let data: Vec<i64> = (-500_000..500_000).collect();
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(10);

    group.bench_function("negative_filter", |b| {
        b.iter(|| {
            ColumnScan::scan_i64(
                black_box(&data),
                black_box(ScanFilter::Lt(0)),
                None,
            )
        })
    });

    group.bench_function("positive_filter", |b| {
        b.iter(|| {
            ColumnScan::scan_i64(
                black_box(&data),
                black_box(ScanFilter::Gte(0)),
                None,
            )
        })
    });

    group.finish();
}

fn bench_f64_scans(c: &mut Criterion) {
    let mut group = c.benchmark_group("f64_scans");

    let data: Vec<f64> = (0..100_000).map(|x| x as f64 * 0.5).collect();
    group.throughput(Throughput::Elements(100_000));

    group.bench_function("eq_filter", |b| {
        b.iter(|| {
            ColumnScan::scan_f64(
                black_box(&data),
                black_box(ScanFilter::Eq(25000.0)),
                None,
            )
        })
    });

    group.bench_function("range_filter", |b| {
        b.iter(|| {
            ColumnScan::scan_f64(
                black_box(&data),
                black_box(ScanFilter::Range(10000.0, 20000.0)),
                None,
            )
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_scan_1m_rows,
    bench_batch_processing,
    bench_different_filters,
    bench_simd_vs_scalar,
    bench_i64_scans,
    bench_f64_scans
);

criterion_main!(benches);
