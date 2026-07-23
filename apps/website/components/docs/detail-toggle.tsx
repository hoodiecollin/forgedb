"use client";

import { useAtom } from "jotai";
import { AlignLeft, AlignJustify } from "lucide-react";
import { detailAtom, type DetailLevel } from "@/lib/atoms";
import { cn } from "@/lib/utils";

const OPTIONS: { value: DetailLevel; label: string; icon: typeof AlignLeft }[] = [
  { value: "terse", label: "Terse", icon: AlignLeft },
  { value: "detailed", label: "Detailed", icon: AlignJustify },
];

/**
 * The reader's global verbosity switch. Sets the sticky `detailAtom`; every
 * disclosure block on the page follows (see `TierDisclosure`). Rendered only on
 * pages that actually carry deeper blocks — the doc page computes that and omits
 * the control otherwise, so it never appears where it would do nothing.
 */
export function DetailToggle() {
  const [detail, setDetail] = useAtom(detailAtom);
  return (
    <div
      role="radiogroup"
      aria-label="Reading detail"
      className="inline-flex items-center gap-0.5 rounded-lg border border-border/60 bg-muted/40 p-0.5 text-sm"
    >
      {OPTIONS.map(({ value, label, icon: Icon }) => {
        const active = detail === value;
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => setDetail(value)}
            className={cn(
              "flex items-center gap-1.5 rounded-md px-2.5 py-1 transition-colors",
              active
                ? "bg-background font-medium text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon className="size-3.5" />
            {label}
          </button>
        );
      })}
    </div>
  );
}
