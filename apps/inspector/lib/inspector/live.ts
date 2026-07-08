/**
 * Live lens (#13) client — talks to a running ForgeDB-generated axum API.
 *
 * The wire contract is exactly what the generator emits (see
 * crates/codegen/src/api.rs); this client hardcodes those shapes and NEVER
 * invents a predicate language of its own — the filter set is the generated
 * closed set (equality on scalar fields), derived from the parsed schema in
 * `data-source`/`atoms`. Honest limits baked in here:
 *   • REST exposes list / get-by-id / create only — there is NO update or delete
 *     endpoint (those mutations live in the DB layer, unexposed). So the Live lens
 *     can insert, but not edit or delete, existing rows over the API.
 *   • Filters are exact-match, AND-ed, no operators (`?field=value`).
 *
 * Requests go over Tauri's HTTP/WebSocket plugins (issued from Rust), so they
 * reach any localhost port regardless of the generated server's CORS config.
 */

import type { GridColumn, LiveDeltaKind } from "./types";

/** Model name (PascalCase) → route segment (kebab-case), per `to_kebab_case`. */
export function kebab(model: string): string {
  return model
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")
    .toLowerCase();
}

/** A row as it comes off the wire — snake_case field names, JSON scalars. */
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

/** `GET /api/<model>?filters` → the record array (unwraps `{ data: [...] }`). */
export async function listRows(
  base: string,
  model: string,
  filters: Record<string, string> = {},
): Promise<LiveRow[]> {
  const res = await tauriFetch(`${base}/api/${kebab(model)}${q(filters)}`);
  if (!res.ok) throw new Error(`GET ${model} → ${res.status}`);
  const json = (await res.json()) as { data?: LiveRow[] };
  return json.data ?? [];
}

/** `POST /api/<model>` → the new row id. Insert only — the sole mutation the API exposes. */
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

// ---- live-query subscription (init / added / updated / removed deltas) ----

/** One delta off `/live-query/<model>`. `id` is set only for `Removed`. */
export interface LiveDelta {
  kind: LiveDeltaKind | "Init";
  rows?: LiveRow[];
  row?: LiveRow;
  id?: string;
}

/** A running subscription; call `close()` to disconnect. */
export interface Subscription {
  close: () => void;
}

const DELTA_KIND: Record<string, LiveDelta["kind"]> = {
  init: "Init",
  added: "Added",
  updated: "Updated",
  removed: "Removed",
};

/**
 * Subscribe to `/live-query/<model>?filters`, mapping each wire delta
 * (`{kind:"init",rows}` / `{kind:"added",row}` / … / `{kind:"removed",id}`) to a
 * typed [`LiveDelta`]. `onError` fires on connect/parse failure.
 */
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

/**
 * Grid columns for a model's live rows, derived from the parsed schema's scalar
 * + FK fields (relations-as-collections are excluded — they aren't columns).
 * Keeps the same `GridColumn` shape the grid already renders.
 */
export function liveColumns(
  fields: { name: string; control: string; typeLabel: string }[],
): GridColumn[] {
  return fields
    .filter((f) => !["hasmany", "m2m", "struct"].includes(f.control))
    .map((f) => ({
      k: f.name,
      l: f.name,
      mono: ["uuid", "int", "bigint", "float", "ts", "char"].includes(f.control),
      rel: f.control === "fk",
    }));
}
