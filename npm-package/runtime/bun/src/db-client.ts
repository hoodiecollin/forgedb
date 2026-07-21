// Unified Database Client for ForgeDB
// Supports both FFI (Sprint 24) and HTTP (Sprint 17) modes

import { Database } from "../ffi/Database";

export interface DBClientConfig {
  // Sprint 17: HTTP endpoint
  apiEndpoint?: string;

  // Sprint 24: FFI path
  dataPath?: string;
  readOnly?: boolean;

  // Auto-detect mode
  mode?: "http" | "ffi" | "auto";
}

export interface DBClient {
  get(model: string, id: string): Promise<any>;
  list(model: string, filters?: Record<string, any>, limit?: number, offset?: number): Promise<any[]>;
  query(model: string, query: any): Promise<any[]>;
  getRelations(model: string, id: string, relation: string): Promise<any[]>;
  close?(): void;
}

/**
 * Create a database client
 *
 * Sprint 17 mode (HTTP):
 *   createDBClient({ apiEndpoint: "http://localhost:3000" })
 *
 * Sprint 24 mode (FFI):
 *   createDBClient({ dataPath: "./data", readOnly: true })
 *
 * Auto mode (default):
 *   createDBClient({ mode: "auto" })
 *   - Tries FFI first
 *   - Falls back to HTTP if FFI unavailable
 */
export function createDBClient(config: DBClientConfig = {}): DBClient {
  const mode = config.mode || detectMode(config);

  if (mode === "ffi") {
    return createFFIClient(config);
  } else {
    return createHTTPClient(config);
  }
}

function detectMode(config: DBClientConfig): "http" | "ffi" {
  // If dataPath provided, use FFI
  if (config.dataPath) {
    return "ffi";
  }

  // If apiEndpoint provided, use HTTP
  if (config.apiEndpoint) {
    return "http";
  }

  // Default to FFI (Sprint 24+)
  return "ffi";
}

/**
 * FFI-based client (Sprint 24)
 */
function createFFIClient(config: DBClientConfig): DBClient {
  const dataPath = config.dataPath || process.env.FORGEDB_DATA || "./data";
  const db = new Database(dataPath, {
    readOnly: config.readOnly ?? true,
    create: true,
  });

  return {
    async get(model: string, id: string): Promise<any> {
      return db.get(model, id);
    },

    async list(model: string, filters?: Record<string, any>, limit: number = 0, offset: number = 0): Promise<any[]> {
      return db.list(model, filters, limit, offset);
    },

    async query(model: string, query: any): Promise<any[]> {
      return db.query(model, query);
    },

    async getRelations(model: string, id: string, relation: string): Promise<any[]> {
      return db.getRelations(model, id, relation);
    },

    close() {
      db.close();
    },
  };
}

/**
 * HTTP-based client (Sprint 17)
 */
function createHTTPClient(config: DBClientConfig): DBClient {
  const apiEndpoint = config.apiEndpoint || process.env.RUST_API_URL || "http://localhost:3000";

  return {
    async get(model: string, id: string): Promise<any> {
      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}/${id}`
      );

      if (!response.ok) {
        if (response.status === 404) {
          return null;
        }
        throw new Error(`HTTP error ${response.status}`);
      }

      return response.json();
    },

    async list(model: string, filters?: Record<string, any>, limit: number = 0, offset: number = 0): Promise<any[]> {
      const params = new URLSearchParams();

      if (filters) {
        params.set("filters", JSON.stringify(filters));
      }
      if (limit > 0) {
        params.set("limit", limit.toString());
      }
      if (offset > 0) {
        params.set("offset", offset.toString());
      }

      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}?${params}`
      );

      if (!response.ok) {
        throw new Error(`HTTP error ${response.status}`);
      }

      return response.json();
    },

    async query(model: string, query: any): Promise<any[]> {
      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}/query`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(query),
        }
      );

      if (!response.ok) {
        throw new Error(`HTTP error ${response.status}`);
      }

      return response.json();
    },

    async getRelations(model: string, id: string, relation: string): Promise<any[]> {
      const response = await fetch(
        `${apiEndpoint}/api/${model.toLowerCase()}/${id}/${relation}`
      );

      if (!response.ok) {
        return [];
      }

      return response.json();
    },
  };
}

// Export Database for direct use
export { Database } from "../ffi/Database";
export { QueryBuilder } from "../ffi/QueryBuilder";
