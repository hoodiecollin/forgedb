"use client";

import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { DEFAULT_ECOSYSTEM, ecosystemAtom } from "@/lib/atoms";

/**
 * Per-ecosystem MDX content. Wrap a fenced install command, a generate command,
 * or a runtime/SDK usage block in
 *
 *   <Eco lang="node">   …Node.js / Bun…   </Eco>
 *   <Eco lang="python"> …Python…          </Eco>
 *   <Eco lang="rust">   …Rust…            </Eco>
 *   <Eco lang="go">     …Go…              </Eco>
 *
 * (space/comma-separated for several, e.g. `lang="rust go"`). Only the block
 * matching the reader's selected ecosystem — the `<EcosystemToggle>` in the doc
 * page chrome — is shown; the rest stay in the DOM but hidden, so deep links and
 * the search index still resolve. Language-agnostic content (`.forge` schema and
 * generated code) is authored normally, outside any `<Eco>`.
 *
 * Before hydration the default (`node`) shows, so SSR and the first client
 * render agree — the preference lives in localStorage, unreadable server-side.
 * (Mirrors the `mounted` guard in the theme toggle.)
 */
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
