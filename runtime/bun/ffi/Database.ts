// High-level Database class for ForgeDB FFI

import { ptr } from "bun:ffi";
import {
  symbols,
  ForgeDBHandle,
  ForgeDBErrorHandle,
  FORGEDB_OPEN_READONLY,
  FORGEDB_OPEN_CREATE,
  FORGEDB_ERR_NOT_FOUND,
} from "./forgedb-ffi";
import { DatabaseOptions, ForgeDBError } from "./types";

export class Database {
  private handle: ForgeDBHandle;
  private path: string;
  private closed: boolean = false;

  // Static registry for automatic cleanup
  private static registry = new FinalizationRegistry<ForgeDBHandle>(
    (handle: ForgeDBHandle) => {
      if (handle !== 0) {
        symbols.forgedb_close(handle);
      }
    }
  );

  constructor(path: string, options: DatabaseOptions = {}) {
    this.path = path;

    // Convert options to flags
    let flags = 0;
    if (options.readOnly) flags |= FORGEDB_OPEN_READONLY;
    if (options.create) flags |= FORGEDB_OPEN_CREATE;

    // Open database
    const errorPtr = new BigUint64Array(1);
    const pathBuffer = Buffer.from(path + "\0", "utf8");

    this.handle = symbols.forgedb_open(
      ptr(pathBuffer),
      flags,
      ptr(errorPtr)
    ) as ForgeDBHandle;

    // Check for errors
    if (this.handle === 0) {
      const errorHandle = errorPtr[0];
      if (errorHandle !== 0n) {
        const code = symbols.forgedb_error_code(errorHandle);
        const messagePtr = symbols.forgedb_error_message(errorHandle);
        const message = new TextDecoder().decode(
          new Uint8Array(
            // @ts-ignore
            Bun.FFI.viewSource(messagePtr, 0, 1024)
          ).subarray(0, new Uint8Array(
            // @ts-ignore
            Bun.FFI.viewSource(messagePtr, 0, 1024)
          ).indexOf(0))
        );
        symbols.forgedb_free_error(errorHandle);
        throw new ForgeDBError(code, message);
      } else {
        throw new ForgeDBError(-1, "Failed to open database");
      }
    }

    // Register for automatic cleanup
    Database.registry.register(this, this.handle, this);
  }

  /**
   * Get a single record by ID
   */
  async get<T = any>(model: string, id: string): Promise<T | null> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelBuffer = Buffer.from(model + "\0", "utf8");
    const idBuffer = Buffer.from(id + "\0", "utf8");

    const resultPtr = symbols.forgedb_get(
      this.handle,
      ptr(modelBuffer),
      ptr(idBuffer),
      ptr(errorPtr)
    ) as number;

    if (resultPtr === 0) {
      const errorHandle = errorPtr[0];
      if (errorHandle !== 0n) {
        const code = symbols.forgedb_error_code(Number(errorHandle));

        // NOT_FOUND is not an error, just return null
        if (code === FORGEDB_ERR_NOT_FOUND) {
          symbols.forgedb_free_error(Number(errorHandle));
          return null;
        }

        const messagePtr = symbols.forgedb_error_message(Number(errorHandle));
        const message = this.readCString(messagePtr as number);
        symbols.forgedb_free_error(Number(errorHandle));
        throw new ForgeDBError(code, message);
      }
      return null;
    }

    try {
      const json = this.readCString(resultPtr);
      if (!json) {
        return null;
      }
      return JSON.parse(json) as T;
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * List records with optional filters
   */
  async list<T = any>(
    model: string,
    filters?: Record<string, any>,
    limit: number = 0,
    offset: number = 0
  ): Promise<T[]> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelBuffer = Buffer.from(model + "\0", "utf8");
    const filtersCStr = filters
      ? ptr(Buffer.from(JSON.stringify(filters) + "\0", "utf8"))
      : 0;

    const resultPtr = symbols.forgedb_list(
      this.handle,
      ptr(modelBuffer),
      filtersCStr,
      limit,
      offset,
      ptr(errorPtr)
    ) as number;

    if (resultPtr === 0) {
      this.handleError(errorPtr);
      return [];
    }

    try {
      const json = this.readCString(resultPtr);
      return JSON.parse(json) as T[];
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * Execute complex query
   */
  async query<T = any>(model: string, query: any): Promise<T[]> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelBuffer = Buffer.from(model + "\0", "utf8");
    const queryBuffer = Buffer.from(JSON.stringify(query) + "\0", "utf8");

    const resultPtr = symbols.forgedb_query(
      this.handle,
      ptr(modelBuffer),
      ptr(queryBuffer),
      ptr(errorPtr)
    ) as number;

    if (resultPtr === 0) {
      this.handleError(errorPtr);
      return [];
    }

    try {
      const json = this.readCString(resultPtr);
      return JSON.parse(json) as T[];
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * Get related records
   */
  async getRelations<T = any>(
    model: string,
    id: string,
    relationName: string
  ): Promise<T[]> {
    this.ensureOpen();

    const errorPtr = new BigUint64Array(1);
    const modelBuffer = Buffer.from(model + "\0", "utf8");
    const idBuffer = Buffer.from(id + "\0", "utf8");
    const relationBuffer = Buffer.from(relationName + "\0", "utf8");

    const resultPtr = symbols.forgedb_get_relations(
      this.handle,
      ptr(modelBuffer),
      ptr(idBuffer),
      ptr(relationBuffer),
      ptr(errorPtr)
    ) as number;

    if (resultPtr === 0) {
      this.handleError(errorPtr);
      return [];
    }

    try {
      const json = this.readCString(resultPtr);
      return JSON.parse(json) as T[];
    } finally {
      symbols.forgedb_free_string(resultPtr);
    }
  }

  /**
   * Close the database
   */
  close(): void {
    if (!this.closed && this.handle !== 0) {
      Database.registry.unregister(this);
      symbols.forgedb_close(this.handle);
      this.handle = 0;
      this.closed = true;
    }
  }

  /**
   * Check if database is open
   */
  isOpen(): boolean {
    return !this.closed && this.handle !== 0;
  }

  private ensureOpen(): void {
    if (this.closed || this.handle === 0) {
      throw new Error("Database is closed");
    }
  }

  private handleError(errorPtr: BigUint64Array): void {
    const errorHandle = errorPtr[0];
    if (errorHandle !== 0n) {
      const code = symbols.forgedb_error_code(Number(errorHandle));
      const messagePtr = symbols.forgedb_error_message(Number(errorHandle));
      const message = this.readCString(messagePtr as number);
      symbols.forgedb_free_error(Number(errorHandle));
      throw new ForgeDBError(code, message);
    }
  }

  private readCString(ptr: number): string {
    if (ptr === 0) return "";

    // Use Bun's toArrayBuffer to read from pointer
    // @ts-ignore - Bun FFI
    const buf = Bun.FFI.toArrayBuffer(ptr, 0, 65536);
    const view = new Uint8Array(buf);

    // Find null terminator
    let length = 0;
    while (length < view.length && view[length] !== 0) {
      length++;
    }

    // Decode up to null terminator
    return new TextDecoder().decode(view.subarray(0, length));
  }
}
