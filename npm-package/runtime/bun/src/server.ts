// ForgeDB Bun Runtime Server
// Sprint 24: Component rendering and route handlers with FFI database access

import { createDBClient, type DBClient } from "./db-client";
import { renderToReadableStream } from "react-dom/server";
import * as React from "react";

// Initialize DB client (defaults to FFI mode)
const db = createDBClient({
  mode: process.env.DB_MODE as "http" | "ffi" | "auto" || "auto",
  dataPath: process.env.FORGEDB_DATA || "./data",
  apiEndpoint: process.env.RUST_API_URL,
  readOnly: true,
});

// Component registry
const components: Record<string, React.ComponentType<any>> = {};

// Register a component
export function registerComponent(key: string, component: React.ComponentType<any>) {
  components[key] = component;
}

// Load components from pages directory
async function loadComponents() {
  const pagesDir = new URL("../pages/", import.meta.url);

  try {
    // Dynamically import all page.tsx files
    // Note: In production, this would use a manifest generated at build time
    const glob = new Bun.Glob("**/page.tsx");

    for await (const file of glob.scan({ cwd: pagesDir.pathname })) {
      const modulePath = new URL(file, pagesDir).pathname;
      const module = await import(modulePath);

      // Extract component key from path: user/card/page.tsx -> user-card
      const key = file.replace("/page.tsx", "").replace(/\//g, "-");

      if (module.default) {
        registerComponent(key, module.default);
      }
    }
  } catch (error) {
    console.warn("[Components] Failed to load components:", error);
  }
}

// Server
const server = Bun.serve({
  port: process.env.PORT || 3001,

  async fetch(req: Request): Promise<Response> {
    const url = new URL(req.url);

    // Component rendering: /pages/{model}/{component}/{id}
    if (url.pathname.startsWith("/pages/")) {
      return handleComponentRender(url, req);
    }

    // Route handlers: /routes/{path}
    if (url.pathname.startsWith("/routes/")) {
      return handleRouteExecution(url, req);
    }

    // Health check
    if (url.pathname === "/health") {
      return new Response(JSON.stringify({
        status: "ok",
        db_mode: process.env.DB_MODE || "auto",
        timestamp: new Date().toISOString()
      }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    return new Response("Not Found", { status: 404 });
  },
});

/**
 * Handle component rendering
 *
 * URL format: /pages/{model}/{component}/{id}?relations={relation1,relation2}
 * Example: /pages/user/card/123?relations=posts
 */
async function handleComponentRender(url: URL, req: Request): Promise<Response> {
  const parts = url.pathname.split("/").filter(Boolean);

  if (parts.length < 4) {
    return new Response("Invalid component path", { status: 400 });
  }

  const [_, modelName, componentName, id] = parts;
  const componentKey = `${modelName}-${componentName}`;

  // Check if component exists
  const Component = components[componentKey];
  if (!Component) {
    return new Response(`Component not found: ${componentKey}`, { status: 404 });
  }

  try {
    const start = performance.now();

    // Fetch data
    const data = await db.get(modelName, id);

    if (!data) {
      return new Response("Not Found", { status: 404 });
    }

    // Fetch relations if requested
    const relations: Record<string, any[]> = {};
    const relationParam = url.searchParams.get("relations");

    if (relationParam) {
      const relationNames = relationParam.split(",").map(r => r.trim());

      for (const relationName of relationNames) {
        const relationData = await db.getRelations(modelName, id, relationName);
        relations[relationName] = relationData;
      }
    }

    const duration = performance.now() - start;
    console.log(`[Perf] render(${componentKey}, ${id}): ${duration.toFixed(2)}ms`);

    // Render component
    const stream = await renderToReadableStream(
      React.createElement(Component, { data, relations })
    );

    return new Response(stream, {
      headers: {
        "Content-Type": "text/html",
        "X-Render-Time": `${duration.toFixed(2)}ms`,
      },
    });
  } catch (error: any) {
    console.error(`[Component] Error rendering ${componentKey}:`, error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }
}

/**
 * Handle route execution
 *
 * URL format: /routes/{path}
 * Example: /routes/user/verify (POST) -> routes/user/verify/post.ts
 */
async function handleRouteExecution(url: URL, req: Request): Promise<Response> {
  const path = url.pathname.substring(8); // Remove "/routes/"
  const method = req.method.toLowerCase();

  try {
    // Build handler path: routes/{path}/{method}.ts
    const handlerPath = new URL(`../routes/${path}/${method}.ts`, import.meta.url).pathname;

    // Dynamic import
    const handler = await import(handlerPath);

    if (!handler.default) {
      return new Response("Handler not found", { status: 404 });
    }

    const start = performance.now();

    // Call handler with request and DB client
    const response = await handler.default(req, db);

    const duration = performance.now() - start;
    console.log(`[Route] ${method.toUpperCase()} /routes/${path}: ${duration.toFixed(2)}ms`);

    return response;
  } catch (error: any) {
    if (error.code === "MODULE_NOT_FOUND" || error.message.includes("Cannot find module")) {
      return new Response(`Route not found: ${method.toUpperCase()} /routes/${path}`, {
        status: 404
      });
    }

    console.error(`[Route] Error executing ${method.toUpperCase()} /routes/${path}:`, error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }
}

// Load components on startup
console.log("[Server] Loading components...");
await loadComponents();
console.log(`[Server] Registered ${Object.keys(components).length} components`);

console.log(`[Server] Listening on http://localhost:${server.port}`);
console.log(`[Server] DB Mode: ${process.env.DB_MODE || "auto (FFI)"}`);

// Cleanup on shutdown
process.on("SIGINT", () => {
  console.log("\n[Server] Shutting down...");
  db.close?.();
  process.exit(0);
});

export { db, components };
