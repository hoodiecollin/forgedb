#!/usr/bin/env bun

// Example: Using ForgeDB FFI from Bun

import { Database } from "./ffi/Database";
import { ForgeDBError } from "./ffi/types";
import "./ffi/QueryBuilder";
import { tmpdir } from "os";
import { join } from "path";

async function main() {
  console.log("ForgeDB FFI Example\n");

  // Create a test database
  const dbPath = join(tmpdir(), `forgedb-test-${Date.now()}`);
  console.log(`Opening database at: ${dbPath}`);

  const db = new Database(dbPath, { create: true });

  try {
    console.log("✅ Database opened successfully\n");

    // Test: List empty database
    console.log("📋 Listing all users (should be empty):");
    const users = await db.list("User");
    console.log(`   Found ${users.length} users`);
    console.log(`   Result: ${JSON.stringify(users)}\n`);

    // Test: Get non-existent record
    console.log("🔍 Getting user with ID 1 (should not exist):");
    try {
      const user = await db.get("User", "1");
      if (user === null) {
        console.log("   ✅ User not found (as expected)\n");
      } else {
        console.log(`   Found: ${JSON.stringify(user)}\n`);
      }
    } catch (err) {
      if (err instanceof ForgeDBError) {
        console.log(`   ✅ Error (expected): ${err.message}\n`);
      } else {
        throw err;
      }
    }

    // Test: Query with limit/offset
    console.log("📊 Querying users with limit=10, offset=0:");
    const results = await db.query("User", { limit: 10, offset: 0 });
    console.log(`   Found ${results.length} users\n`);

    // Test: Get relations (should return empty array)
    console.log("🔗 Getting relations for user 1:");
    const relations = await db.getRelations("User", "1", "posts");
    console.log(`   Found ${relations.length} relations`);
    console.log(`   Result: ${JSON.stringify(relations)}\n`);

    // Test: Query builder
    console.log("🔨 Testing query builder:");
    const queryResults = await db
      .queryBuilder("User")
      .limit(5)
      .offset(0)
      .execute();
    console.log(`   Found ${queryResults.length} users\n`);

    console.log("✅ All tests passed!");
  } catch (err) {
    if (err instanceof ForgeDBError) {
      console.error(`❌ ForgeDB Error [${err.code}]: ${err.message}`);
    } else {
      console.error("❌ Error:", err);
    }
    process.exit(1);
  } finally {
    db.close();
    console.log("\n🔒 Database closed");
  }
}

main().catch(console.error);
