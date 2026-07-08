"use client";

/**
 * Live grid rows for the Studio screen (#13). When attached to a real project's
 * running API, fetches the model's rows (`GET /api/<model>?filters`) and then
 * keeps them current from a `/live-query` subscription — init replaces, added/
 * updated upsert by id, removed drops by id. Columns come from the parsed schema.
 *
 * Filters are the composed predicates, which bind to the generated closed set
 * (equality on scalar fields); nothing here parses a predicate language.
 */

import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import {
  apiBaseAtom,
  connectedAtom,
  predicatesAtom,
  projectSourceAtom,
  schemaAtom,
  studioModelAtom,
} from "./atoms";
import { isTauri } from "./data-source";
import {
  type LiveDelta,
  type LiveRow,
  type Subscription,
  liveColumns,
  liveQuery,
  listRows,
} from "./live";
import type { GridColumn } from "./types";

export interface LiveRowsState {
  /** true when the live path is engaged (Tauri + real project + attached) */
  active: boolean;
  loading: boolean;
  error: string | null;
  cols: GridColumn[];
  rows: LiveRow[];
}

const ID = "id";

function applyDelta(rows: LiveRow[], d: LiveDelta): LiveRow[] {
  switch (d.kind) {
    case "Init":
      return d.rows ?? [];
    case "Added":
    case "Updated": {
      if (!d.row) return rows;
      const id = d.row[ID];
      const i = rows.findIndex((r) => r[ID] === id);
      if (i === -1) return [d.row, ...rows];
      const next = rows.slice();
      next[i] = d.row;
      return next;
    }
    case "Removed":
      return rows.filter((r) => String(r[ID]) !== String(d.id));
    default:
      return rows;
  }
}

/** Strip the display quoting the composer stores values with (e.g. `"draft"`). */
function unquote(v: string): string {
  return v.replace(/^"(.*)"$/, "$1");
}

export function useLiveRows(): LiveRowsState {
  const source = useAtomValue(projectSourceAtom);
  const connected = useAtomValue(connectedAtom);
  const model = useAtomValue(studioModelAtom);
  const predicates = useAtomValue(predicatesAtom);
  const base = useAtomValue(apiBaseAtom);
  const schema = useAtomValue(schemaAtom);

  const active = isTauri() && source === "project" && connected;
  const [rows, setRows] = useState<LiveRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const cols = liveColumns(schema[model] ?? []);
  const filterKey = JSON.stringify(
    Object.fromEntries(
      predicates
        .filter((p) => p.val !== "")
        .map((p) => [p.field, unquote(p.val)]),
    ),
  );

  useEffect(() => {
    if (!active) {
      setRows([]);
      setError(null);
      return;
    }
    const filters = JSON.parse(filterKey) as Record<string, string>;
    let cancelled = false;
    let sub: Subscription | null = null;
    setLoading(true);
    setError(null);

    listRows(base, model, filters)
      .then((r) => {
        if (!cancelled) {
          setRows(r);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
          setLoading(false);
        }
      });

    liveQuery(base, model, filters, (d) => {
      if (!cancelled) setRows((cur) => applyDelta(cur, d));
    }).then((s) => {
      if (cancelled) s.close();
      else sub = s;
    });

    return () => {
      cancelled = true;
      sub?.close();
    };
  }, [active, base, model, filterKey]);

  return { active, loading, error, cols, rows };
}
