"use client";

import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { isConnectedScreenLiveAtom, streamAtom } from "./atoms";
import type { LiveDeltaKind } from "./types";

const KINDS: LiveDeltaKind[] = ["Added", "Added", "Updated", "Removed"];
const NAMES = ["ada", "lin", "max", "sol", "ivy", "ken", "zoe", "raf"];
const pick = <T,>(xs: T[]) => xs[Math.floor(Math.random() * xs.length)]!;

/**
 * Mock change feed. While a Live surface is on screen (and attached), it appends
 * a typed delta every ~1.8s so the live tail / dashboards visibly stream. The
 * real feed (#13) subscribes to the generated WS endpoint instead.
 */
export function useLiveTicker() {
  const live = useAtomValue(isConnectedScreenLiveAtom);
  const setStream = useSetAtom(streamAtom);

  useEffect(() => {
    if (!live) return;
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
  }, [live, setStream]);
}
