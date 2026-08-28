import type { GridColumn, LiveDeltaKind } from "./types";
export function kebab(model: string): string {
  return model
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")
    .toLowerCase();
}
export type LiveRow = Record<string, unknown>;
function q(filters: Record<string, string>): string {
  const entries = Object.entries(filters).filter(([, v]) => v !== "");
  if (entries.length === 0) return "";
  const usp = new URLSearchParams();
  for (const [k, v] of entries) usp.set(k, v);
  return `?${usp.toString()}`;
}
async function tauriFetch(
  url: string,
  init?: { method?: string; body?: string },
): Promise<Response> {
  const { fetch } = await import("@tauri-apps/plugin-http");
  return fetch(url, {
    method: init?.method ?? "GET",
    headers: init?.body ? { "Content-Type": "application/json" } : undefined,
    body: init?.body,
  });
}
function withAsOf(
  filters: Record<string, string>,
  asOf?: number,
): Record<string, string> {
  return asOf === undefined ? filters : { ...filters, as_of: String(asOf) };
}
export async function listRows(
  base: string,
  model: string,
  filters: Record<string, string> = {},
  asOf?: number,
): Promise<LiveRow[]> {
  const res = await tauriFetch(`${base}/api/${kebab(model)}${q(withAsOf(filters, asOf))}`);
  if (!res.ok) throw new Error(`GET ${model} → ${res.status}`);
  const json = (await res.json()) as { data?: LiveRow[] };
  return json.data ?? [];
}
export async function getRow(
  base: string,
  model: string,
  id: string,
  asOf?: number,
): Promise<LiveRow | null> {
  const res = await tauriFetch(
    `${base}/api/${kebab(model)}/${encodeURIComponent(id)}${q(withAsOf({}, asOf))}`,
  );
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`GET ${model}/${id} → ${res.status}`);
  return (await res.json()) as LiveRow;
}
export async function getSnapshotToken(
  base: string,
): Promise<Record<string, number>> {
  const res = await tauriFetch(`${base}/snapshot`);
  if (!res.ok) throw new Error(`GET /snapshot → ${res.status}`);
  const json = (await res.json()) as { watermarks?: Record<string, number> };
  return json.watermarks ?? {};
}
export async function createRow(
  base: string,
  model: string,
  body: Record<string, unknown>,
): Promise<string> {
  const res = await tauriFetch(`${base}/api/${kebab(model)}`, {
    method: "POST",
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST ${model} → ${res.status}`);
  const json = (await res.json()) as { id?: string };
  return String(json.id ?? "");
}
export async function updateRow(
  base: string,
  model: string,
  id: string,
  body: Record<string, unknown>,
): Promise<void> {
  const res = await tauriFetch(`${base}/api/${kebab(model)}/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`PUT ${model}/${id} → ${res.status}`);
}
export async function deleteRow(base: string, model: string, id: string): Promise<void> {
  const res = await tauriFetch(`${base}/api/${kebab(model)}/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`DELETE ${model}/${id} → ${res.status}`);
}
export function buildRecordBody(
  fields: {
    name: string;
    control: string;
    mods: string[];
  }[],
  base: LiveRow,
  values: Record<string, string>,
  nulls: Record<string, boolean>,
  bools: Record<string, string>,
): LiveRow {
  const out: LiveRow = { ...base };
  for (const f of fields) {
    const c = f.control;
    if (c === "bool") {
      const b = bools[f.name] ?? (base[f.name] as string | undefined);
      out[f.name] = b === "true" ? true : b === "false" ? false : null;
      continue;
    }
    if (["hasmany", "m2m", "struct"].includes(c)) {
      out[f.name] = base[f.name] ?? null;
      continue;
    }
    if (nulls[f.name] === true) {
      out[f.name] = null;
      continue;
    }
    const raw = values[f.name];
    if (raw === undefined) continue;
    if (c === "int" || c === "float" || c === "ts") {
      const n = Number(raw);
      out[f.name] = Number.isFinite(n) ? n : raw;
    } else {

      out[f.name] = raw;
    }
  }
  return out;
}
export interface LiveDelta {
  kind: LiveDeltaKind | "Init";
  rows?: LiveRow[];
  row?: LiveRow;
  id?: string;
}
export interface Subscription {
  close: () => void;
}
const DELTA_KIND: Record<string, LiveDelta["kind"]> = {
  init: "Init",
  added: "Added",
  updated: "Updated",
  removed: "Removed",
};
export async function liveQuery(
  base: string,
  model: string,
  filters: Record<string, string>,
  onDelta: (d: LiveDelta) => void,
  onError?: (e: unknown) => void,
): Promise<Subscription> {
  const WebSocket = (await import("@tauri-apps/plugin-websocket")).default;
  const ws = base.replace(/^http/, "ws");
  const url = `${ws}/live-query/${kebab(model)}${q(filters)}`;
  let closed = false;
  try {
    const socket = await WebSocket.connect(url);
    socket.addListener((msg) => {
      if (closed) return;
      if (msg.type !== "Text") return;
      try {
        const raw = JSON.parse(msg.data as string) as {
          kind: string;
          rows?: LiveRow[];
          row?: LiveRow;
          id?: string;
        };
        const kind = DELTA_KIND[raw.kind];
        if (!kind) return;
        onDelta({ kind, rows: raw.rows, row: raw.row, id: raw.id });
      } catch (e) {
        onError?.(e);
      }
    });
    return {
      close: () => {
        closed = true;
        void socket.disconnect();
      },
    };
  } catch (e) {
    onError?.(e);
    return { close: () => {} };
  }
}
export function liveColumns(
  fields: { name: string; control: string; typeLabel: string }[],
): GridColumn[] {
  return fields
    .filter((f) => !["hasmany", "m2m", "struct"].includes(f.control))
    .map((f) => ({
      k: f.name,
      l: f.name,
      mono: ["uuid", "int", "bigint", "float", "ts", "bytes"].includes(f.control),
      rel: f.control === "fk",
    }));
}
