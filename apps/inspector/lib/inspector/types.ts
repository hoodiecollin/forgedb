export type Mod = "+" | "&" | "^" | "?";
export type FieldControl =
  | "uuid"
  | "string"
  | "text"
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
  v: string;
  label: string;
}
export interface StructSubField {
  name: string;
  value: string;
  ph: string;
}
export interface Field {
  name: string;
  typeLabel: string;
  mods: Mod[];
  directive?: string;
  control: FieldControl;
  value?: string;
  placeholder?: string;
  min?: number;
  max?: number;
  len?: number;
  msVal?: string;
  humanVal?: string;
  default?: string;
  fkTarget?: string;
  fkCurrent?: string;
  fkOptions?: FkOption[];
  structFields?: StructSubField[];
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
  dataMB: string;
  offMB: string;
  reclaim: string;
  idxCount: string;
  x: number;
  y: number;
}
export interface Relation {
  kind: string;
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
