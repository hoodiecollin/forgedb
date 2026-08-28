import type {
  Field,
  FieldControl,
  Health,
  Mod,
  Model,
  Relation,
  StructSubField,
} from "./types";
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const FILTERABLE_CONTROLS = new Set<FieldControl>([
  "uuid",
  "string",
  "text",
  "int",
  "bigint",
  "float",
  "bool",
  "ts",
  "bytes",
]);
export function filterableFields(fields: Field[]): Field[] {
  return fields.filter((f) => FILTERABLE_CONTROLS.has(f.control));
}
export interface ProjectStructure {
  dbName: string;
  models: Model[];
  rel: Record<string, Relation[]>;
  schema: Record<string, Field[]>;
  source: "mock" | "project";
  schemaPath?: string;
  hasStats: boolean;
}

interface DirectiveDto {
  name: string;
  params: string[];
}
interface FieldDto {
  name: string;
  kind: string;
  auto: boolean;
  unique: boolean;
  indexed: boolean;
  nullable: boolean;
  bytesLen: number | null;
  arrayLen: number | null;
  fulltext: boolean;
  computed: boolean;
  materialized: boolean;
  relTarget: string | null;
  structName: string | null;
  directives: DirectiveDto[];
}
interface ModelStatsDto {
  totalRows: number;
  activeRows: number;
  deletedRows: number;
  deadSpaceRatio: number;
  totalDiskBytes: number;
  usedBytes: number;
  deadBytes: number;
}
interface ModelDto {
  name: string;
  softDelete: boolean;
  compositeIndexes: string[][];
  indexCount: number;
  fields: FieldDto[];
  stats: ModelStatsDto | null;
}
interface StructDto {
  name: string;
  fields: FieldDto[];
}
interface ProjectDto {
  dbName: string;
  schemaPath: string;
  dataDir: string | null;
  hasStats: boolean;
  models: ModelDto[];
  structs: StructDto[];
}
function directiveText(d: DirectiveDto): string {
  if (d.params.length === 0) return `@${d.name}`;
  const args = d.params
    .map((p) => (/^-?\d+(\.\d+)?$/.test(p) ? p : JSON.stringify(p)))
    .join(", ");
  return `@${d.name}(${args})`;
}
function numericDirective(f: FieldDto, name: string): number | undefined {
  const d = f.directives.find((x) => x.name === name);
  if (!d || d.params[0] === undefined) return undefined;
  const n = Number(d.params[0]);
  return Number.isFinite(n) ? n : undefined;
}
function controlFor(f: FieldDto): FieldControl {
  switch (f.kind) {
    case "uuid":
      return "uuid";
    case "u32":
    case "i32":
      return "int";
    case "u64":
    case "i64":
      return "bigint";
    case "f64":
      return "float";
    case "bool":
      return "bool";
    case "timestamp":
      return "ts";
    case "bytes":
      return "bytes";
    case "required_ref":
    case "optional_ref":
      return "fk";
    case "struct":
      return "struct";
    case "one_to_many":
      return "hasmany";
    case "many_to_many":
      return "m2m";
    case "string":
      return f.fulltext || f.directives.some((d) => d.name === "length")
        ? "text"
        : "string";
    default:
      return "string";
  }
}
function typeLabelFor(f: FieldDto): string {
  switch (f.kind) {
    case "bytes":
      return `bytes(${f.bytesLen ?? 0})`;
    case "fixed_array":
      return `[array; ${f.arrayLen ?? 0}]`;
    case "struct":
      return f.structName ? `struct ${f.structName}` : "struct";
    case "required_ref":
      return `*${f.relTarget ?? ""}`;
    case "optional_ref":
      return `?${f.relTarget ?? ""}`;
    case "one_to_many":
    case "many_to_many":
      return `[${f.relTarget ?? ""}]`;
    default:
      return f.kind;
  }
}

function modsFor(f: FieldDto): Mod[] {
  const mods: Mod[] = [];
  if (f.auto) mods.push("+");
  if (f.unique) mods.push("&");
  if (f.indexed) mods.push("^");
  if (f.nullable) mods.push("?");
  return mods;
}
function mapField(
  f: FieldDto,
  structs: Record<string, StructDto>,
): Field {
  const control = controlFor(f);
  const directive =
    f.directives.length > 0
      ? f.directives.map(directiveText).join(" ")
      : undefined;
  const field: Field = {
    name: f.name,
    typeLabel: typeLabelFor(f),
    mods: modsFor(f),
    control,
    directive,
  };
  if (control === "int") {
    field.min = numericDirective(f, "min");
    field.max = numericDirective(f, "max");
  }
  if (control === "bytes") field.len = f.bytesLen ?? undefined;
  if (control === "fk") field.fkTarget = f.relTarget ?? undefined;
  if (control === "hasmany" || control === "m2m")
    field.target = f.relTarget ?? undefined;
  const def = f.directives.find((d) => d.name === "default");
  if (def && def.params[0] !== undefined) field.default = def.params[0];

  if (control === "struct" && f.structName) {
    const s = structs[f.structName];
    if (s) {
      field.structFields = s.fields.map<StructSubField>((sf) => ({
        name: sf.name,
        value: "",
        ph: sf.name,
      }));
    }
  }
  return field;
}
function healthFor(deadPct: number): Health {
  if (deadPct >= 25) return "danger";
  if (deadPct >= 10) return "warn";
  return "ok";
}

function gridPosition(index: number, total: number): { x: number; y: number } {
  const cols = Math.max(1, Math.ceil(Math.sqrt(total)));
  const col = index % cols;
  const row = Math.floor(index / cols);
  return { x: 48 + col * 210, y: 44 + row * 150 };
}
function mapModel(m: ModelDto, index: number, total: number): Model {
  const pos = gridPosition(index, total);
  const s = m.stats;
  const deadPct = s ? Math.round(s.deadSpaceRatio * 100) : 0;
  const mb = (bytes: number) => (bytes / 1_000_000).toFixed(1);

  return {
    key: m.name,
    rows: s ? s.activeRows.toLocaleString() : "—",
    deadPct,
    deadCount: s ? s.deletedRows.toLocaleString() : "—",
    health: s ? healthFor(deadPct) : "ok",
    dataMB: s ? mb(s.usedBytes) : "—",
    offMB: s ? mb(s.deadBytes) : "—",
    reclaim: s ? s.deletedRows.toLocaleString() : "—",
    idxCount: m.indexCount.toString(),
    x: pos.x,
    y: pos.y,
  };
}
function relationsFor(m: ModelDto): Relation[] {
  const out: Relation[] = [];
  for (const f of m.fields) {
    if (!f.relTarget) continue;
    switch (f.kind) {
      case "required_ref":
        out.push({ kind: "*FK", label: `${f.name} → ${f.relTarget}`, to: f.relTarget, k: "fk" });
        break;
      case "optional_ref":
        out.push({ kind: "?FK", label: `${f.name} → ${f.relTarget}`, to: f.relTarget, k: "fk" });
        break;
      case "one_to_many":
        out.push({ kind: "has-many", label: `${f.name} → ${f.relTarget}`, to: f.relTarget, k: "hm" });
        break;
      case "many_to_many":
        out.push({ kind: "↔ M2M", label: `${f.name} ↔ ${f.relTarget}`, to: f.relTarget, k: "m2m" });
        break;
    }
  }
  return out;
}
function mapProject(dto: ProjectDto): ProjectStructure {
  const structs: Record<string, StructDto> = {};
  for (const s of dto.structs) structs[s.name] = s;
  const models: Model[] = dto.models.map((m, i) =>
    mapModel(m, i, dto.models.length),
  );
  const rel: Record<string, Relation[]> = {};
  const schema: Record<string, Field[]> = {};
  for (const m of dto.models) {
    rel[m.name] = relationsFor(m);
    schema[m.name] = m.fields.map((f) => mapField(f, structs));
  }
  return {
    dbName: dto.dbName,
    models,
    rel,
    schema,
    source: "project",
    schemaPath: dto.schemaPath,
    hasStats: dto.hasStats,
  };
}

export async function loadProject(
  schemaPath: string,
  dataDir?: string,
): Promise<ProjectStructure> {
  const { invoke } = await import("@tauri-apps/api/core");
  const dto = await invoke<ProjectDto>("load_project", {
    schemaPath,
    dataDir: dataDir ?? null,
  });
  return mapProject(dto);
}

export async function loadStartupProject(): Promise<ProjectStructure | null> {
  const { invoke } = await import("@tauri-apps/api/core");
  const startup = await invoke<{
    schemaPath: string;
    dataDir: string | null;
  } | null>("startup_project");
  if (!startup) return null;
  return loadProject(startup.schemaPath, startup.dataDir ?? undefined);
}

export async function openProject(): Promise<ProjectStructure | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "ForgeDB schema", extensions: ["forge"] }],
    title: "Open a .forge schema",
  });
  if (typeof picked !== "string") return null;
  const dataDir = await open({
    multiple: false,
    directory: true,
    title: "Select the data directory (optional — cancel to skip)",
  });
  return loadProject(picked, typeof dataDir === "string" ? dataDir : undefined);
}
