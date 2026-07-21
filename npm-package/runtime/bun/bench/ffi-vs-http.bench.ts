// Performance Benchmarks: FFI vs HTTP
// Sprint 24: Validate 10-100x performance improvement

import { Database } from "../ffi/Database";

const RUST_API_URL = process.env.RUST_API_URL || "http://localhost:3000";
const DATA_PATH = process.env.FORGEDB_DATA || "/tmp/forgedb-bench-data";

// Setup FFI database
const db = new Database(DATA_PATH, { readOnly: true, create: true });

// Warm up (ensure DB and HTTP API are ready)
console.log("Warming up...");
try {
  await db.get("User", "1");
} catch (e) {
  console.log("FFI warm-up completed (no data yet)");
}

try {
  await fetch(`${RUST_API_URL}/health`);
  console.log("HTTP API ready");
} catch (e) {
  console.log("HTTP API not available, skipping HTTP benchmarks");
}

// Benchmark utilities
interface BenchResult {
  name: string;
  ops: number;
  avgMs: number;
  minMs: number;
  maxMs: number;
}

async function bench(name: string, fn: () => Promise<void>, iterations: number = 1000): Promise<BenchResult> {
  const times: number[] = [];

  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    await fn();
    const duration = performance.now() - start;
    times.push(duration);
  }

  const sum = times.reduce((a, b) => a + b, 0);
  const avg = sum / times.length;
  const min = Math.min(...times);
  const max = Math.max(...times);

  return {
    name,
    ops: iterations,
    avgMs: avg,
    minMs: min,
    maxMs: max,
  };
}

function printResult(result: BenchResult) {
  console.log(`\n${result.name}:`);
  console.log(`  Operations: ${result.ops}`);
  console.log(`  Average:    ${result.avgMs.toFixed(3)}ms`);
  console.log(`  Min:        ${result.minMs.toFixed(3)}ms`);
  console.log(`  Max:        ${result.maxMs.toFixed(3)}ms`);
}

function printComparison(ffiResult: BenchResult, httpResult: BenchResult) {
  const improvement = httpResult.avgMs / ffiResult.avgMs;
  console.log(`\n${"=".repeat(60)}`);
  console.log(`Improvement: ${improvement.toFixed(2)}x faster`);
  console.log(`FFI:  ${ffiResult.avgMs.toFixed(3)}ms`);
  console.log(`HTTP: ${httpResult.avgMs.toFixed(3)}ms`);
  console.log(`${"=".repeat(60)}\n`);
}

// Benchmarks
console.log("\n" + "=".repeat(60));
console.log("PERFORMANCE BENCHMARKS: FFI vs HTTP");
console.log("=".repeat(60));

// 1. Get single record
console.log("\n### Benchmark 1: Get Single Record (1000 ops)");

const ffiGetResult = await bench(
  "FFI: get single record",
  async () => {
    await db.get("User", "1");
  },
  1000
);
printResult(ffiGetResult);

try {
  const httpGetResult = await bench(
    "HTTP: get single record",
    async () => {
      await fetch(`${RUST_API_URL}/api/users/1`).then(r => r.json().catch(() => null));
    },
    1000
  );
  printResult(httpGetResult);
  printComparison(ffiGetResult, httpGetResult);
} catch (e) {
  console.log("\nHTTP benchmark skipped (API not available)");
}

// 2. List 10 records
console.log("\n### Benchmark 2: List 10 Records (500 ops)");

const ffiList10Result = await bench(
  "FFI: list 10 records",
  async () => {
    await db.list("User", {}, 10, 0);
  },
  500
);
printResult(ffiList10Result);

try {
  const httpList10Result = await bench(
    "HTTP: list 10 records",
    async () => {
      await fetch(`${RUST_API_URL}/api/users?limit=10`).then(r => r.json().catch(() => []));
    },
    500
  );
  printResult(httpList10Result);
  printComparison(ffiList10Result, httpList10Result);
} catch (e) {
  console.log("\nHTTP benchmark skipped (API not available)");
}

// 3. List 100 records
console.log("\n### Benchmark 3: List 100 Records (200 ops)");

const ffiList100Result = await bench(
  "FFI: list 100 records",
  async () => {
    await db.list("User", {}, 100, 0);
  },
  200
);
printResult(ffiList100Result);

try {
  const httpList100Result = await bench(
    "HTTP: list 100 records",
    async () => {
      await fetch(`${RUST_API_URL}/api/users?limit=100`).then(r => r.json().catch(() => []));
    },
    200
  );
  printResult(httpList100Result);
  printComparison(ffiList100Result, httpList100Result);
} catch (e) {
  console.log("\nHTTP benchmark skipped (API not available)");
}

// 4. Query with filters
console.log("\n### Benchmark 4: Query with Filters (500 ops)");

const ffiQueryResult = await bench(
  "FFI: query with filters",
  async () => {
    await db.list("User", { verified: true }, 10, 0);
  },
  500
);
printResult(ffiQueryResult);

try {
  const httpQueryResult = await bench(
    "HTTP: query with filters",
    async () => {
      const params = new URLSearchParams({ filters: JSON.stringify({ verified: true }), limit: "10" });
      await fetch(`${RUST_API_URL}/api/users?${params}`).then(r => r.json().catch(() => []));
    },
    500
  );
  printResult(httpQueryResult);
  printComparison(ffiQueryResult, httpQueryResult);
} catch (e) {
  console.log("\nHTTP benchmark skipped (API not available)");
}

// 5. Get relations
console.log("\n### Benchmark 5: Get Relations (500 ops)");

const ffiRelationsResult = await bench(
  "FFI: get relations",
  async () => {
    await db.getRelations("User", "1", "posts");
  },
  500
);
printResult(ffiRelationsResult);

try {
  const httpRelationsResult = await bench(
    "HTTP: get relations",
    async () => {
      await fetch(`${RUST_API_URL}/api/users/1/posts`).then(r => r.json().catch(() => []));
    },
    500
  );
  printResult(httpRelationsResult);
  printComparison(ffiRelationsResult, httpRelationsResult);
} catch (e) {
  console.log("\nHTTP benchmark skipped (API not available)");
}

// Summary
console.log("\n" + "=".repeat(60));
console.log("BENCHMARK SUMMARY");
console.log("=".repeat(60));
console.log(`\nFFI Performance:`);
console.log(`  Get single:    ${ffiGetResult.avgMs.toFixed(3)}ms`);
console.log(`  List 10:       ${ffiList10Result.avgMs.toFixed(3)}ms`);
console.log(`  List 100:      ${ffiList100Result.avgMs.toFixed(3)}ms`);
console.log(`  Query:         ${ffiQueryResult.avgMs.toFixed(3)}ms`);
console.log(`  Relations:     ${ffiRelationsResult.avgMs.toFixed(3)}ms`);

// Cleanup
db.close();
console.log("\nBenchmarks complete!");
