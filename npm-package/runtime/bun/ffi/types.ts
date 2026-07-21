// TypeScript types for ForgeDB FFI

export interface DatabaseOptions {
  readOnly?: boolean;
  create?: boolean;
}

export interface QueryOptions {
  filters?: Record<string, any>;
  sort?: string[];
  limit?: number;
  offset?: number;
}

export class ForgeDBError extends Error {
  constructor(
    public code: number,
    message: string
  ) {
    super(message);
    this.name = "ForgeDBError";
  }
}
