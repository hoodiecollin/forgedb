"use client";

import { useEffect, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useAtom } from "jotai";
import {
  DEFAULT_ECOSYSTEM,
  ECOSYSTEMS,
  ecosystemAtom,
  isEcosystem,
  type Ecosystem,
} from "@/lib/atoms";
import { cn } from "@/lib/utils";

const LABELS: Record<Ecosystem, string> = {
  node: "Node / Bun",
  python: "Python",
  rust: "Rust",
  go: "Go",
};

/**
 * The reader's global ecosystem switch (Node/Bun · Python · Rust · Go). Sets the
 * sticky `ecosystemAtom`; every `<Eco>` block on the page follows, swapping the
 * suggested install command and the runtime/SDK usage examples to the selected
 * language. Schema and generated-code samples never change — they're
 * language-agnostic.
 *
 * Persisted two ways: `atomWithStorage` (localStorage, sticky across pages and
 * sessions) and a `?eco=` query param (shareable deep links). A `?eco=` present
 * on load wins over the stored preference. Reads `useSearchParams`, so it must
 * render inside a Suspense boundary (see the doc page). Rendered only on pages
 * that actually carry `<Eco>` blocks — the doc page computes that and omits the
 * control otherwise, so it never appears where it would do nothing.
 */
export function EcosystemToggle() {
  const [eco, setEco] = useAtom(ecosystemAtom);
  const [mounted, setMounted] = useState(false);
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useRouter();

  // A `?eco=` deep link overrides the stored preference on first load only.
  useEffect(() => {
    setMounted(true);
    const q = searchParams.get("eco");
    if (q && isEcosystem(q) && q !== eco) setEco(q);
    // Run once on mount; intentionally not reacting to later eco/searchParams changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const select = (value: Ecosystem) => {
    setEco(value);
    // Reflect the choice in the URL (shareable) without scrolling or a nav.
    const params = new URLSearchParams(searchParams.toString());
    params.set("eco", value);
    router.replace(`${pathname}?${params.toString()}`, { scroll: false });
  };

  // Before hydration, render the default so SSR and the first client render
  // agree (localStorage isn't readable server-side).
  const active = mounted ? eco : DEFAULT_ECOSYSTEM;

  return (
    <div
      role="radiogroup"
      aria-label="Language ecosystem"
      className="inline-flex items-center gap-0.5 rounded-lg border border-border/60 bg-muted/40 p-0.5 text-sm"
    >
      {ECOSYSTEMS.map((value) => {
        const isActive = active === value;
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={isActive}
            onClick={() => select(value)}
            className={cn(
              "rounded-md px-2.5 py-1 transition-colors",
              isActive
                ? "bg-background font-medium text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {LABELS[value]}
          </button>
        );
      })}
    </div>
  );
}
