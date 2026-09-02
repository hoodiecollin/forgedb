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

export function EcosystemToggle() {
  const [eco, setEco] = useAtom(ecosystemAtom);
  const [mounted, setMounted] = useState(false);
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useRouter();
  useEffect(() => {
    setMounted(true);
    const q = searchParams.get("eco");
    if (q && isEcosystem(q) && q !== eco) setEco(q);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const select = (value: Ecosystem) => {
    setEco(value);
    const params = new URLSearchParams(searchParams.toString());
    params.set("eco", value);
    router.replace(`${pathname}?${params.toString()}`, { scroll: false });
  };
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
