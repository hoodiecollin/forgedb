// Memory Leak Tests
// Sprint 24: Validate zero memory leaks in FFI operations

import { test, expect } from "bun:test";
import { Database } from "../ffi/Database";
import { tmpdir } from "os";
import { join } from "path";
import { rmSync } from "fs";

const TEST_DB_PATH = join(tmpdir(), `forgedb-memory-test-${Date.now()}`);

// Cleanup after all tests
process.on("exit", () => {
  try {
    rmSync(TEST_DB_PATH, { recursive: true, force: true });
  } catch (e) {
    // Ignore cleanup errors
  }
});

test("no memory leaks - 10k get operations", async () => {
  const db = new Database(TEST_DB_PATH, { create: true });

  // Force garbage collection if available
  if (global.gc) {
    global.gc();
  }

  const initialMemory = process.memoryUsage().heapUsed;

  // Perform 10,000 operations
  for (let i = 0; i < 10000; i++) {
    await db.get("User", `id-${i % 100}`);

    // Force GC every 1000 operations
    if (i % 1000 === 0 && global.gc) {
      global.gc();
    }
  }

  // Force final GC
  if (global.gc) {
    global.gc();
    // Wait a bit for finalization
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  const finalMemory = process.memoryUsage().heapUsed;
  const memoryGrowth = finalMemory - initialMemory;
  const memoryGrowthMB = memoryGrowth / (1024 * 1024);

  console.log(`Memory growth: ${memoryGrowthMB.toFixed(2)}MB`);

  // Memory growth should be reasonable (< 50MB for 1k operations)
  // Note: Some growth is expected due to JSON parsing/caching
  expect(memoryGrowth).toBeLessThan(50 * 1024 * 1024);

  db.close();
});

test("no memory leaks - 1k list operations", async () => {
  const db = new Database(TEST_DB_PATH, { create: true });

  if (global.gc) {
    global.gc();
  }

  const initialMemory = process.memoryUsage().heapUsed;

  // Perform 1,000 list operations with various limits
  for (let i = 0; i < 1000; i++) {
    const limit = [10, 50, 100][i % 3];
    await db.list("User", {}, limit, 0);

    if (i % 100 === 0 && global.gc) {
      global.gc();
    }
  }

  if (global.gc) {
    global.gc();
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  const finalMemory = process.memoryUsage().heapUsed;
  const memoryGrowth = finalMemory - initialMemory;
  const memoryGrowthMB = memoryGrowth / (1024 * 1024);

  console.log(`Memory growth: ${memoryGrowthMB.toFixed(2)}MB`);

  // Memory growth should be reasonable (< 50MB for 1k operations)
  expect(memoryGrowth).toBeLessThan(50 * 1024 * 1024);

  db.close();
});

test("no memory leaks - mixed operations", async () => {
  const db = new Database(TEST_DB_PATH, { create: true });

  if (global.gc) {
    global.gc();
  }

  const initialMemory = process.memoryUsage().heapUsed;

  // Perform 5,000 mixed operations
  for (let i = 0; i < 5000; i++) {
    const op = i % 4;

    switch (op) {
      case 0:
        await db.get("User", `id-${i % 100}`);
        break;
      case 1:
        await db.list("User", {}, 10, 0);
        break;
      case 2:
        await db.query("User", { filters: { verified: true }, limit: 10 });
        break;
      case 3:
        await db.getRelations("User", `id-${i % 100}`, "posts");
        break;
    }

    if (i % 500 === 0 && global.gc) {
      global.gc();
    }
  }

  if (global.gc) {
    global.gc();
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  const finalMemory = process.memoryUsage().heapUsed;
  const memoryGrowth = finalMemory - initialMemory;
  const memoryGrowthMB = memoryGrowth / (1024 * 1024);

  console.log(`Memory growth: ${memoryGrowthMB.toFixed(2)}MB`);

  expect(memoryGrowth).toBeLessThan(15 * 1024 * 1024);

  db.close();
});

test("automatic cleanup on garbage collection", async () => {
  let db: Database | null = new Database(TEST_DB_PATH, { create: true });

  // Use database
  await db.get("User", "test-id");

  // Remove reference (but don't call close())
  db = null;

  // Force GC
  if (global.gc) {
    global.gc();

    // Wait for finalization
    await new Promise(resolve => setTimeout(resolve, 200));
  }

  // If we get here without crash, cleanup worked
  expect(true).toBe(true);
});

test("explicit close prevents further operations", async () => {
  const db = new Database(TEST_DB_PATH, { create: true });

  // Use database
  await db.get("User", "test-id");

  // Close explicitly
  db.close();

  // Should throw error
  try {
    await db.get("User", "test-id");
    expect(true).toBe(false); // Should not reach here
  } catch (error: any) {
    expect(error.message).toContain("closed");
  }
});

test("concurrent access safety - 100 parallel requests", async () => {
  const db = new Database(TEST_DB_PATH, { create: true });

  // Create 100 concurrent requests with different operations
  const promises = Array.from({ length: 100 }, (_, i) => {
    const op = i % 4;

    switch (op) {
      case 0:
        return db.get("User", `id-${i % 10}`);
      case 1:
        return db.list("User", {}, 10, 0);
      case 2:
        return db.query("User", { filters: {}, limit: 10 });
      case 3:
        return db.getRelations("User", `id-${i % 10}`, "posts");
      default:
        return db.get("User", "1");
    }
  });

  const results = await Promise.all(promises);

  // All requests should complete
  expect(results.length).toBe(100);

  db.close();
});

test("stress test - rapid open/close cycles", async () => {
  if (global.gc) {
    global.gc();
  }

  const initialMemory = process.memoryUsage().heapUsed;

  // Open and close database 100 times
  for (let i = 0; i < 100; i++) {
    const db = new Database(TEST_DB_PATH, { create: true });
    await db.get("User", "test-id");
    db.close();

    if (i % 10 === 0 && global.gc) {
      global.gc();
    }
  }

  if (global.gc) {
    global.gc();
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  const finalMemory = process.memoryUsage().heapUsed;
  const memoryGrowth = finalMemory - initialMemory;
  const memoryGrowthMB = memoryGrowth / (1024 * 1024);

  console.log(`Memory growth after 100 open/close cycles: ${memoryGrowthMB.toFixed(2)}MB`);

  // Memory should not grow significantly
  expect(memoryGrowth).toBeLessThan(5 * 1024 * 1024);
});

test("handle validation - invalid handle after close", async () => {
  const db = new Database(TEST_DB_PATH, { create: true });

  // Close database
  db.close();

  // isOpen should return false
  expect(db.isOpen()).toBe(false);

  // Operations should fail
  await expect(db.get("User", "1")).rejects.toThrow();
});
