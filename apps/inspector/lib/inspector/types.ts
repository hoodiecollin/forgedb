/**
 * Domain types for the ForgeDB Inspector.
 *
 * These model ForgeDB's *schema surface* as the inspector sees it — the same
 * shapes whether they arrive from the at-rest Structure lens (parsed `.forge` +
 * manifests) or the Live lens (a running generated API). In this increment they
 * are populated from mock data; later increments swap the source, not the types.
 *
 * Fidelity note (see docs/forgedb-inspector-design-review.md): ForgeDB has NO
 * `text` type. `control: "text"` is a *rendering heuristic* over a `string`
 * field carrying `@length`/`@fulltext` — `typeLabel` stays `"string"`.
 */

/** Field modifier prefixes, exactly as the schema language defines them. */
export type Mod = "+" | "&" | "^" | "?";
//                 auto  unique index nullable

/** Which editor/display control a field maps to. */
export type FieldControl =
  | "uuid"
  | "string"
  | "text" // heuristic over string (@length/@fulltext) — NOT a schema type
  | "int"
  | "bigint"
  | "float"
  | "bool"
  | "ts"
  | "bytes"
  | "fk"
  | "struct"
  | "hasmany"
  | "m2m";

export type RelationKind = "fk" | "hm" | "m2m";
export type Health = "ok" | "warn" | "danger";

export interface FkOption {
  /** opaque row key */
  v: string;
  /** human label for the row */
  label: string;
}

export interface StructSubField {
  name: string;
  value: string;
  ph: string;
}

/** One field of a model, with everything a type-aware control needs. */
export interface Field {
  name: string;
  /** e.g. "uuid", "string", "u32", "bytes(8)", "*Org", "[Post]" */
  typeLabel: string;
  mods: Mod[];
  /** semantic-only directive marker(s), e.g. "@email", "@min(0) @max(120)" */
  directive?: string;
  control: FieldControl;
  value?: string;
  placeholder?: string;
  /** int bounds from @min/@max */
  min?: number;
  max?: number;
  /** bytes(N) length */
  len?: number;
  /** timestamp (Unix ms) */
  msVal?: string;
  humanVal?: string;
  /** bool default ("true"|"false") — semantic-only marker */
  default?: string;
  /** fk */
  fkTarget?: string;
  fkCurrent?: string;
  fkOptions?: FkOption[];
  /** struct */
  structFields?: StructSubField[];
  /** hasmany / m2m */
  target?: string;
  relCount?: string;
  chips?: { label: string }[];
}

export interface Model {
  key: string;
  rows: string;
  deadPct: number;
  deadCount: string;
  health: Health;
  /** at-rest storage stats (MB) */
  dataMB: string;
  offMB: string;
  reclaim: string;
  idxCount: string;
  /** relation-graph node position (hand-placed until the graph lib lands, #67) */
  x: number;
  y: number;
}

export interface Relation {
  kind: string; // display label, e.g. "*FK", "has-many", "↔ M2M"
  label: string;
  to: string;
  k: RelationKind;
}

export interface GridColumn {
  k: string;
  l: string;
  mono?: boolean;
  rel?: boolean;
}

export interface GridData {
  cols: GridColumn[];
  rows: Record<string, string | null>[];
}

/**
 * A filter predicate. `idx` = the field carries `^` (indexed) so the predicate
 * resolves fast; otherwise it's a full scan and the UI warns. The set of
 * available predicates is the generated closed set — never a free-form parser
 * (design-review correction #4).
 */
export interface Predicate {
  field: string;
  op: string;
  val: string;
  idx: boolean;
}

export type Screen = "atlas" | "studio" | "console" | "dashboards";
export type Lens = "structure" | "live";
export type ConsoleTab = "q1" | "live" | "snap";
export type LiveDeltaKind = "Added" | "Updated" | "Removed";

export interface SavedQuery {
  name: string;
  model: string;
}
export interface Snapshot {
  name: string;
  time: string;
}
