/**
 * Live lens (#13) client — talks to a running ForgeDB-generated axum API.
 *
 * The wire contract is exactly what the generator emits (see
 * crates/codegen/src/api.rs); this client hardcodes those shapes and NEVER
 * invents a predicate language of its own — the filter set is the generated
 * closed set (equality on scalar fields), derived from the parsed schema in
 * `data-source`/`atoms`. Honest limits baked in here:
 *   • REST exposes list / get-by-id / create / replace (PUT) / delete (DELETE)
 *     (#69). Update is a WHOLE-RECORD replace over the generated superseding-
 *     version append (#66), never a field-level patch.
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

/**
 * Merge the point-in-time `as_of` watermark (#85) into a filter map. The
 * watermark is an opaque row-count position — the wire token the generated
 * snapshot endpoints accept (`?as_of=<n>`); `undefined` reads live (newest).
 */
function withAsOf(
  filters: Record<string, string>,
  asOf?: number,
): Record<string, string> {
  return asOf === undefined ? filters : { ...filters, as_of: String(asOf) };
}

/**
 * `GET /api/<model>?filters` → the record array (unwraps `{ data: [...] }`).
 * With `asOf` (a row-count watermark, #85) the generated endpoint reads
 * `all_at` — the rows as of that point in time — instead of the live newest.
 */
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

/**
 * `GET /api/<model>/<id>` → the full record, or null on 404. With `asOf` the
 * generated endpoint reads `get_at` (the version visible at that watermark);
 * a row not yet present at the watermark reads as 404 → null (#85).
 */
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

/**
 * `GET /snapshot` → the current per-model row-count **watermarks** (#85): a
 * `{ "<Model>": <rowCount> }` map captured atomically on the server. The client
 * freezes this as a snapshot token and passes a model's watermark back as `asOf`
 * to read that model as of this instant. Keys are PascalCase model names.
 */
export async function getSnapshotToken(
  base: string,
): Promise<Record<string, number>> {
  const res = await tauriFetch(`${base}/snapshot`);
  if (!res.ok) throw new Error(`GET /snapshot → ${res.status}`);
  const json = (await res.json()) as { watermarks?: Record<string, number> };
  return json.watermarks ?? {};
}

/** `POST /api/<model>` → the new row id. */
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

/**
 * `PUT /api/<model>/<id>` — whole-record replace (#69). `body` must be the full
 * record (the generated update is superseding-version append, not a partial
 * patch). Resolves on 200; throws on 404 (absent id) / 422 (invalid payload).
 */
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

/** `DELETE /api/<model>/<id>` — tombstone the row (#69). Resolves on 204; throws on 404. */
export async function deleteRow(base: string, model: string, id: string): Promise<void> {
  const res = await tauriFetch(`${base}/api/${kebab(model)}/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`DELETE ${model}/${id} → ${res.status}`);
}

/**
 * Build the JSON body for a create/replace, overlaying the editor's per-field
 * values onto a `base` row. `base` is the existing live row (for a replace, so
 * every generated struct field is present) or `{}` (for a create). Coercion is
 * by control: numbers for int/float/ts, strings for bigint/uuid/bytes/string,
 * tri-state bool, explicit `null` for nulled scalars. Relation fields
 * (hasmany/m2m/struct) that the generated struct carries as `()`/optional are
 * passed through from `base` (or null) untouched — the inspector doesn't author
 * them here. Auto (`+`) fields keep their base value (a create sends a
 * placeholder the generated `insert` overwrites).
 */
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
      // Not authored here; keep whatever base carried (or null).
      out[f.name] = base[f.name] ?? null;
      continue;
    }
    if (nulls[f.name] === true) {
      out[f.name] = null;
      continue;
    }
    const raw = values[f.name];
    if (raw === undefined) continue; // untouched — keep base value
    if (c === "int" || c === "float" || c === "ts") {
      const n = Number(raw);
      out[f.name] = Number.isFinite(n) ? n : raw;
    } else {
      // bigint/uuid/bytes/string/text/fk — string on the wire
      out[f.name] = raw;
    }
  }
  return out;
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
      mono: ["uuid", "int", "bigint", "float", "ts", "bytes"].includes(f.control),
      rel: f.control === "fk",
    }));
}
