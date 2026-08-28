"use client";

import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { ChevronRight, Telescope, Wrench } from "lucide-react";
import { detailAtom } from "@/lib/atoms";
import { cn } from "@/lib/utils";

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
  summary?: string;
  children: ReactNode;
}) {
  const detail = useAtomValue(detailAtom);
  const globalOpen = detail === "detailed";
  const [open, setOpen] = useState(globalOpen);
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
