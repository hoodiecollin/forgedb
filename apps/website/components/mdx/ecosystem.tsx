"use client";

import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { DEFAULT_ECOSYSTEM, ecosystemAtom } from "@/lib/atoms";

export function Eco({ lang, children }: { lang: string; children: ReactNode }) {
  const eco = useAtomValue(ecosystemAtom);
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  const active = mounted ? eco : DEFAULT_ECOSYSTEM;
  const langs = lang.split(/[\s,]+/).filter(Boolean);
  const show = langs.includes(active);
  return (
    <div data-eco={lang} hidden={!show}>
      {children}
    </div>
  );
}
