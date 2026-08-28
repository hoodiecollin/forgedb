"use client";

import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  apiBaseAtom,
  isConnectedScreenLiveAtom,
  projectSourceAtom,
  streamAtom,
  studioModelAtom,
} from "./atoms";
import { isTauri } from "./data-source";
import { type LiveDelta, type Subscription, liveQuery } from "./live";
import type { LiveDeltaKind } from "./types";

const KINDS: LiveDeltaKind[] = ["Added", "Added", "Updated", "Removed"];
const NAMES = ["ada", "lin", "max", "sol", "ivy", "ken", "zoe", "raf"];
const pick = <T,>(xs: T[]) => xs[Math.floor(Math.random() * xs.length)]!;

function summarize(row: Record<string, unknown> | undefined): string {
  if (!row) return "(row)";
  const label =
    (row.title as string) ??
    (row.name as string) ??
    (row.body as string) ??
    (row.email as string) ??
    (row.id as string) ??
    "row";
  return String(label).slice(0, 60);
}

export function useLiveTicker() {
  const live = useAtomValue(isConnectedScreenLiveAtom);
  const source = useAtomValue(projectSourceAtom);
  const model = useAtomValue(studioModelAtom);
  const base = useAtomValue(apiBaseAtom);
  const setStream = useSetAtom(streamAtom);
  const real = isTauri() && source === "project";
  useEffect(() => {
    if (!live) return;
    if (real) {
      let sub: Subscription | null = null;
      let cancelled = false;
      const push = (d: LiveDelta) => {
        if (cancelled || d.kind === "Init") return;
        const row = d.row;
        const id = String(row?.id ?? d.id ?? "");
        const ts = new Date().toLocaleTimeString("en-US", { hour12: false });
        setStream((s) =>
          [
            {
              kind: d.kind as LiveDeltaKind,
              id,
              who: model,
              text: `${model} ${summarize(row)}`,
              ts,
            },
            ...s,
          ].slice(0, 7),
        );
      };
      liveQuery(base, model, {}, push).then((s) => {
        if (cancelled) s.close();
        else sub = s;
      });
      return () => {
        cancelled = true;
        sub?.close();
      };
    }
    const iv = setInterval(() => {
      const kind = pick(KINDS);
      const id =
        Math.random().toString(16).slice(2, 6) +
        "…" +
        Math.random().toString(16).slice(2, 4);
      const who = pick(NAMES);
      const ts = new Date().toLocaleTimeString("en-US", { hour12: false });
      setStream((s) =>
        [{ kind, id, who, text: `Comment ${id} · by ${who}`, ts }, ...s].slice(
          0,
          7,
        ),
      );
    }, 1800);
    return () => clearInterval(iv);
  }, [live, real, base, model, setStream]);
}
