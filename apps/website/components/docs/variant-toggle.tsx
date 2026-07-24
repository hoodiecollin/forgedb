"use client";

import Link from "next/link";
import { useSetAtom } from "jotai";
import { AlignLeft, AlignJustify } from "lucide-react";
import { detailAtom } from "@/lib/atoms";
import { cn } from "@/lib/utils";

/**
 * The Build-C variant switch. Unlike {@link DetailToggle} (which expands blocks
 * in place on a Build-B page), a Build-C page keeps its terse and detailed
 * tellings as two separate SSR'd routes — `/docs/x/` and `/docs/x/detailed/` —
 * so both are fully static, independently indexable, and free of duplicate
 * heading anchors. This control navigates between the two siblings, carrying
 * the current heading hash so switching voice keeps your place. It also syncs
 * the global `detailAtom` so a reader's terse/detailed preference stays
 * consistent with Build-B pages.
 */
export function VariantToggle({
  terseHref,
  detailedHref,
  active,
}: {
  terseHref: string;
  detailedHref: string;
  active: "terse" | "detailed";
}) {
  const setDetail = useSetAtom(detailAtom);
  const options = [
    { value: "terse" as const, label: "Terse", icon: AlignLeft, href: terseHref },
    { value: "detailed" as const, label: "Detailed", icon: AlignJustify, href: detailedHref },
  ];

  return (
    <div
      role="radiogroup"
      aria-label="Reading detail"
      className="inline-flex items-center gap-0.5 rounded-lg border border-border/60 bg-muted/40 p-0.5 text-sm"
    >
      {options.map(({ value, label, icon: Icon, href }) => {
        const isActive = active === value;
        return (
          <Link
            key={value}
            href={href}
            role="radio"
            aria-checked={isActive}
            scroll={false}
            onClick={() => setDetail(value)}
            onMouseDown={() => {
              // Carry the reader's current section across the voice switch so
              // the sibling route lands on the same heading (anchors match).
              const hash =
                typeof window !== "undefined" ? window.location.hash : "";
              if (hash) {
                const el = document.querySelector<HTMLAnchorElement>(
                  `a[href="${href}"]`,
                );
                if (el) el.href = href.replace(/\/$/, "/") + hash;
              }
            }}
            className={cn(
              "flex items-center gap-1.5 rounded-md px-2.5 py-1 transition-colors",
              isActive
                ? "bg-background font-medium text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon className="size-3.5" />
            {label}
          </Link>
        );
      })}
    </div>
  );
}
