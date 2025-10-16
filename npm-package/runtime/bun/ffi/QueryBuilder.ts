// Type-safe query builder for ForgeDB

import { Database } from "./Database";

export class QueryBuilder<T = any> {
  private filters: Record<string, any> = {};
  private sortFields: string[] = [];
  private _limit?: number;
  private _offset?: number;

  constructor(
    private db: Database,
    private model: string
  ) {}

  /**
   * Add equality filter
   */
  where(field: string, value: any): this {
    this.filters[field] = value;
    return this;
  }

  /**
   * Add comparison filter
   */
  whereLt(field: string, value: number): this {
    this.filters[field] = { lt: value };
    return this;
  }

  whereLte(field: string, value: number): this {
    this.filters[field] = { lte: value };
    return this;
  }

  whereGt(field: string, value: number): this {
    this.filters[field] = { gt: value };
    return this;
  }

  whereGte(field: string, value: number): this {
    this.filters[field] = { gte: value };
    return this;
  }

  /**
   * Add IN filter
   */
  whereIn(field: string, values: any[]): this {
    this.filters[field] = { in: values };
    return this;
  }

  /**
   * Add sort field
   */
  orderBy(field: string, direction: "asc" | "desc" = "asc"): this {
    const sortField = direction === "desc" ? `-${field}` : field;
    this.sortFields.push(sortField);
    return this;
  }

  /**
   * Set limit
   */
  limit(n: number): this {
    this._limit = n;
    return this;
  }

  /**
   * Set offset
   */
  offset(n: number): this {
    this._offset = n;
    return this;
  }

  /**
   * Execute query and return results
   */
  async execute(): Promise<T[]> {
    const query: any = {};

    if (Object.keys(this.filters).length > 0) {
      query.filters = this.filters;
    }

    if (this.sortFields.length > 0) {
      query.sort = this.sortFields;
    }

    if (this._limit !== undefined) {
      query.limit = this._limit;
    }

    if (this._offset !== undefined) {
      query.offset = this._offset;
    }

    return this.db.query<T>(this.model, query);
  }

  /**
   * Get first result
   */
  async first(): Promise<T | null> {
    const results = await this.limit(1).execute();
    return results.length > 0 ? results[0] : null;
  }

  /**
   * Count results (without fetching data)
   */
  async count(): Promise<number> {
    const results = await this.execute();
    return results.length;
  }
}

// Extend Database class with query builder
declare module "./Database" {
  interface Database {
    queryBuilder<T>(model: string): QueryBuilder<T>;
  }
}

Database.prototype.queryBuilder = function <T>(
  this: Database,
  model: string
): QueryBuilder<T> {
  return new QueryBuilder<T>(this, model);
};
