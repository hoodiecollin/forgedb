"use client";

import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { ChevronRight, Telescope, Wrench } from "lucide-react";
import { detailAtom } from "@/lib/atoms";
import { cn } from "@/lib/utils";

/**
 * The MDX vocabulary for progressive disclosure (Build B). A page's terse body
 * is Tier 1 — always visible. Authors wrap the deeper registers in these:
 *
 *   <DiveDeeper>            …Tier 2, mechanism-at-concept-level (scale 5-6)…
 *   <ImplementationDetails> …Tier 3, the manual (scale 7-10)…
 *
 * Both collapse by default and expand together when the reader flips the global
 * verbosity toggle to "detailed" — that flip is an expand-all. A reader can also
 * open one block on its own; the next global flip re-syncs everything, clearing
 * the local override. Content stays in the DOM when collapsed (search, deep
 * links, and the dev rewrite tool's source-mapping all need it there).
 */

type TierStyle = {
  tier: "deeper" | "technical";
  icon: typeof Telescope;
  wrap: string;
  head: string;
  iconCls: string;
};

const DEEPER: TierStyle = {
  tier: "deeper",
  icon: Telescope,
  wrap: "border-primary/25 bg-primary/[0.04]",
  head: "text-primary/90 hover:text-primary",
  iconCls: "text-primary/70",
};

const TECHNICAL: TierStyle = {
  tier: "technical",
  icon: Wrench,
  wrap: "border-border/70 bg-muted/30",
  head: "text-muted-foreground hover:text-foreground",
  iconCls: "text-muted-foreground/70",
};

function TierDisclosure({
  style,
  label,
  summary,
  children,
}: {
  style: TierStyle;
  label: string;
  /** One-line hint shown next to the label when collapsed. */
  summary?: string;
  children: ReactNode;
}) {
  const detail = useAtomValue(detailAtom);
  const globalOpen = detail === "detailed";
  const [open, setOpen] = useState(globalOpen);

  // The global toggle is an expand-all / collapse-all: whenever the reader's
  // verbosity preference flips, re-sync this block to it (dropping any local
  // override). Individual clicks below set only the local state.
  useEffect(() => setOpen(globalOpen), [globalOpen]);

  const Icon = style.icon;
  return (
    <div
      data-tier={style.tier}
      className={cn("my-5 overflow-hidden rounded-lg border", style.wrap)}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm font-medium transition-colors",
          style.head,
        )}
      >
        <ChevronRight
          className={cn(
            "size-4 shrink-0 transition-transform duration-200",
            open && "rotate-90",
          )}
        />
        <Icon className={cn("size-4 shrink-0", style.iconCls)} />
        <span>{label}</span>
        {summary && !open ? (
          <span className="truncate text-xs font-normal text-muted-foreground/80">
            — {summary}
          </span>
        ) : null}
      </button>
      <div
        hidden={!open}
        className="border-t border-inherit px-4 pt-1 pb-4 text-[15px] [&>:first-child]:mt-0 [&>:last-child]:mb-0"
      >
        {children}
      </div>
    </div>
  );
}

/** Tier 2 — the mechanism, one level down from the terse body. */
export function DiveDeeper({
  summary,
  children,
}: {
  summary?: string;
  children: ReactNode;
}) {
  return (
    <TierDisclosure style={DEEPER} label="Dive deeper" summary={summary}>
      {children}
    </TierDisclosure>
  );
}

/** Tier 3 — the manual. Optional; only where there's genuinely novel depth. */
export function ImplementationDetails({
  summary,
  children,
}: {
  summary?: string;
  children: ReactNode;
}) {
  return (
    <TierDisclosure style={TECHNICAL} label="Implementation details" summary={summary}>
      {children}
    </TierDisclosure>
  );
}
