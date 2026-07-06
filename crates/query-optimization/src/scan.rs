// Columnar scanning with SIMD operations and batch processing
//
// This module implements efficient column scanning with:
// - SIMD operations for numeric filters (using platform-specific intrinsics)
// - Batch processing (1024 rows at a time)
// - Early termination for LIMIT queries

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Batch size for columnar scanning
pub const BATCH_SIZE: usize = 1024;

/// Filter operation for column scanning
#[derive(Debug, Clone, PartialEq)]
pub enum ScanFilter<T> {
    Eq(T),
    Ne(T),
    Gt(T),
    Gte(T),
    Lt(T),
    Lte(T),
    Range(T, T),
}

/// Result of a column scan operation
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Row indices that match the filter
    pub matching_rows: Vec<usize>,
    /// Number of rows scanned
    pub rows_scanned: usize,
    /// Whether scan was terminated early (due to LIMIT)
    pub early_termination: bool,
}

/// Column scanner for efficient batch processing
pub struct ColumnScan;

impl ColumnScan {
    /// Scan u64 column with SIMD optimization
    ///
    /// # Safety
    /// Uses x86_64 SIMD intrinsics which are safe on x86_64 platforms
    pub fn scan_u64(
        column_data: &[u64],
        filter: ScanFilter<u64>,
        limit: Option<usize>,
    ) -> ScanResult {
        let mut matching_rows = Vec::with_capacity(BATCH_SIZE);
        let mut rows_scanned = 0;
        let target_limit = limit.unwrap_or(usize::MAX);

        // Process in batches of BATCH_SIZE
        for batch_start in (0..column_data.len()).step_by(BATCH_SIZE) {
            let batch_end = (batch_start + BATCH_SIZE).min(column_data.len());
            let batch = &column_data[batch_start..batch_end];

            // Use SIMD for batch processing
            let batch_matches = Self::scan_u64_batch_simd(batch, &filter, batch_start);
            rows_scanned += batch.len();

            // Collect matching rows
            for row in batch_matches {
                matching_rows.push(row);

                // Early termination if we hit the limit
                if matching_rows.len() >= target_limit {
                    return ScanResult {
                        matching_rows,
                        rows_scanned,
                        early_termination: true,
                    };
                }
            }
        }

        ScanResult {
            matching_rows,
            rows_scanned,
            early_termination: false,
        }
    }

    /// SIMD-optimized batch scanning for u64 values
    ///
    /// Uses AVX2 instructions when available, falls back to scalar processing
    #[inline]
    fn scan_u64_batch_simd(batch: &[u64], filter: &ScanFilter<u64>, offset: usize) -> Vec<usize> {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                // SAFETY: We've checked for AVX2 support
                return unsafe { Self::scan_u64_batch_avx2(batch, filter, offset) };
            }
        }

        // Fallback to scalar processing
        Self::scan_u64_batch_scalar(batch, filter, offset)
    }

    /// AVX2-optimized scanning for u64 values
    ///
    /// Processes 4 u64 values at a time using 256-bit SIMD registers
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    unsafe fn scan_u64_batch_avx2(
        batch: &[u64],
        filter: &ScanFilter<u64>,
        offset: usize,
    ) -> Vec<usize> {
        let mut matches = Vec::new();
        const SIMD_WIDTH: usize = 4; // 256 bits / 64 bits = 4 u64 values

        match filter {
            ScanFilter::Eq(value) => {
                let compare_vec = _mm256_set1_epi64x(*value as i64);

                // Process 4 values at a time
                let mut i = 0;
                while i + SIMD_WIDTH <= batch.len() {
                    let data_vec = _mm256_loadu_si256(batch.as_ptr().add(i) as *const __m256i);
                    let cmp_result = _mm256_cmpeq_epi64(data_vec, compare_vec);
                    let mask = _mm256_movemask_pd(_mm256_castsi256_pd(cmp_result));

                    // Extract matching indices
                    for j in 0..SIMD_WIDTH {
                        if (mask & (1 << j)) != 0 {
                            matches.push(offset + i + j);
                        }
                    }

                    i += SIMD_WIDTH;
                }

                // Handle remaining elements
                for j in i..batch.len() {
                    if batch[j] == *value {
                        matches.push(offset + j);
                    }
                }
            }
            ScanFilter::Gt(value) => {
                // _mm256_cmpgt_epi64 performs a SIGNED 64-bit comparison.  For u64
                // operands, values ≥ 2^63 have their sign bit set and compare as
                // negative under signed semantics, producing wrong results.
                //
                // Fix: XOR both operands with 0x8000_0000_0000_0000 (flip the sign
                // bit) before comparing.  This maps u64 ordering to i64 ordering:
                //   u64 min (0)       → i64 min (i64::MIN)
                //   u64 max (u64::MAX) → i64 max (i64::MAX)
                // so the signed cmpgt result matches the unsigned ordering.
                const BIAS: u64 = 1u64 << 63;
                let biased_value = (*value ^ BIAS) as i64;
                let compare_vec = _mm256_set1_epi64x(biased_value);
                let bias_vec = _mm256_set1_epi64x(BIAS as i64);

                let mut i = 0;
                while i + SIMD_WIDTH <= batch.len() {
                    let data_vec = _mm256_loadu_si256(batch.as_ptr().add(i) as *const __m256i);
                    // Bias the data lane to convert unsigned to signed ordering.
                    let biased_data = _mm256_xor_si256(data_vec, bias_vec);
                    let cmp_result = _mm256_cmpgt_epi64(biased_data, compare_vec);
                    let mask = _mm256_movemask_pd(_mm256_castsi256_pd(cmp_result));

                    for j in 0..SIMD_WIDTH {
                        if (mask & (1 << j)) != 0 {
                            matches.push(offset + i + j);
                        }
                    }

                    i += SIMD_WIDTH;
                }

                for j in i..batch.len() {
                    if batch[j] > *value {
                        matches.push(offset + j);
                    }
                }
            }
            _ => {
                // For other operations, fall back to scalar
                return Self::scan_u64_batch_scalar(batch, filter, offset);
            }
        }

        matches
    }

    /// Scalar fallback for batch scanning
    #[inline]
    fn scan_u64_batch_scalar(batch: &[u64], filter: &ScanFilter<u64>, offset: usize) -> Vec<usize> {
        let mut matches = Vec::new();

        for (i, &value) in batch.iter().enumerate() {
            let is_match = match filter {
                ScanFilter::Eq(f) => value == *f,
                ScanFilter::Ne(f) => value != *f,
                ScanFilter::Gt(f) => value > *f,
                ScanFilter::Gte(f) => value >= *f,
                ScanFilter::Lt(f) => value < *f,
                ScanFilter::Lte(f) => value <= *f,
                ScanFilter::Range(min, max) => value >= *min && value <= *max,
            };

            if is_match {
                matches.push(offset + i);
            }
        }

        matches
    }

    /// Scan i64 column with SIMD optimization
    pub fn scan_i64(
        column_data: &[i64],
        filter: ScanFilter<i64>,
        limit: Option<usize>,
    ) -> ScanResult {
        let mut matching_rows = Vec::with_capacity(BATCH_SIZE);
        let mut rows_scanned = 0;
        let target_limit = limit.unwrap_or(usize::MAX);

        for batch_start in (0..column_data.len()).step_by(BATCH_SIZE) {
            let batch_end = (batch_start + BATCH_SIZE).min(column_data.len());
            let batch = &column_data[batch_start..batch_end];

            let batch_matches = Self::scan_i64_batch_scalar(batch, &filter, batch_start);
            rows_scanned += batch.len();

            for row in batch_matches {
                matching_rows.push(row);

                if matching_rows.len() >= target_limit {
                    return ScanResult {
                        matching_rows,
                        rows_scanned,
                        early_termination: true,
                    };
                }
            }
        }

        ScanResult {
            matching_rows,
            rows_scanned,
            early_termination: false,
        }
    }

    #[inline]
    fn scan_i64_batch_scalar(batch: &[i64], filter: &ScanFilter<i64>, offset: usize) -> Vec<usize> {
        let mut matches = Vec::new();

        for (i, &value) in batch.iter().enumerate() {
            let is_match = match filter {
                ScanFilter::Eq(f) => value == *f,
                ScanFilter::Ne(f) => value != *f,
                ScanFilter::Gt(f) => value > *f,
                ScanFilter::Gte(f) => value >= *f,
                ScanFilter::Lt(f) => value < *f,
                ScanFilter::Lte(f) => value <= *f,
                ScanFilter::Range(min, max) => value >= *min && value <= *max,
            };

            if is_match {
                matches.push(offset + i);
            }
        }

        matches
    }

    /// Scan f64 column (no SIMD due to floating-point complexity)
    pub fn scan_f64(
        column_data: &[f64],
        filter: ScanFilter<f64>,
        limit: Option<usize>,
    ) -> ScanResult {
        let mut matching_rows = Vec::with_capacity(BATCH_SIZE);
        let mut rows_scanned = 0;
        let target_limit = limit.unwrap_or(usize::MAX);

        for batch_start in (0..column_data.len()).step_by(BATCH_SIZE) {
            let batch_end = (batch_start + BATCH_SIZE).min(column_data.len());
            let batch = &column_data[batch_start..batch_end];

            for (i, &value) in batch.iter().enumerate() {
                rows_scanned += 1;

                let is_match = match &filter {
                    ScanFilter::Eq(f) => value == *f,
                    ScanFilter::Ne(f) => value != *f,
                    ScanFilter::Gt(f) => value > *f,
                    ScanFilter::Gte(f) => value >= *f,
                    ScanFilter::Lt(f) => value < *f,
                    ScanFilter::Lte(f) => value <= *f,
                    ScanFilter::Range(min, max) => value >= *min && value <= *max,
                };

                if is_match {
                    matching_rows.push(batch_start + i);

                    if matching_rows.len() >= target_limit {
                        return ScanResult {
                            matching_rows,
                            rows_scanned,
                            early_termination: true,
                        };
                    }
                }
            }
        }

        ScanResult {
            matching_rows,
            rows_scanned,
            early_termination: false,
        }
    }
}

