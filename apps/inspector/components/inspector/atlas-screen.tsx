"use client";

/**
 * Atlas — the schema as a navigable map. Two lenses make the brief's biggest
 * split a top-level control: Structure (reads files at rest) vs Live (typed data
 * over the running API). The graph is hand-placed SVG for now; the real
 * force/DAG layout lands via #67 (@xyflow/react + @dagrejs/dagre).
 */

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Boxes, Activity, TriangleAlert } from "lucide-react";
import {
  browseModelAtom,
  connectedAtom,
  lensAtom,
  screenAtom,
  selModelAtom,
} from "@/lib/inspector/atoms";
import { MODELS, REL, SCHEMA } from "@/lib/inspector/mock";
import { Button } from "@/components/ui/button";
import { NotAttached } from "./not-attached";
import { cn } from "@/lib/utils";

const dotColor = (h: string) =>
  h === "warn" ? "bg-warn" : h === "danger" ? "bg-danger" : "bg-ok";
const relColor = (k: string) =>
  k === "m2m" ? "text-info border-info/50" : k === "hm" ? "text-ok border-ok/50" : "text-muted-foreground border-border";

export function AtlasScreen() {
  const [lens, setLens] = useAtom(lensAtom);
  const [selModel, setSelModel] = useAtom(selModelAtom);
  const connected = useAtomValue(connectedAtom);
  const browse = useSetAtom(browseModelAtom);
  const setScreen = useSetAtom(screenAtom);

  const sel = MODELS.find((m) => m.key === selModel) ?? MODELS[0]!;
  const structure = lens === "structure";

  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col">
        {/* lens toolbar */}
        <div className="flex flex-none items-center gap-3 border-b border-border px-4 py-2.5">
          <div className="flex items-center gap-0.5 rounded-[9px] border border-border bg-muted p-0.5">
            <LensButton active={structure} onClick={() => setLens("structure")} icon={Boxes} label="Structure · at rest" />
            <LensButton active={!structure} onClick={() => setLens("live")} icon={Activity} label="Live · attached" />
          </div>
          <div className="text-[12.5px] text-muted-foreground">
            {structure
              ? "Reads the database files directly — works without the app running."
              : "Typed data over the running API server — rows, queries, edits, live changes."}
          </div>
          <div className="ml-auto flex items-center gap-3">
            <span
              title="Design spike (#67): the relation graph is hand-composed. Real layout = @xyflow/react + @dagrejs/dagre."
              className="inline-flex items-center gap-1.5 rounded-md border border-dashed border-warn/45 bg-warn/10 px-2 py-0.5 font-mono text-[10.5px] font-semibold text-warn"
            >
              <TriangleAlert className="size-3" /> SPIKE · graph lib (#67)
            </span>
            <span className="font-mono text-[12px] text-muted-foreground">
              5 models · 7 relations
            </span>
          </div>
        </div>

        {/* graph canvas */}
        <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto bg-[radial-gradient(circle_at_1px_1px,color-mix(in_oklab,var(--foreground)_8%,transparent)_1px,transparent_0)] bg-[length:22px_22px]">
          <div className="relative h-[420px] w-[620px] flex-none">
            <svg
              className="absolute inset-0 h-[420px] w-[620px]"
              fill="none"
              stroke="color-mix(in oklab,var(--muted-foreground) 55%,transparent)"
              strokeWidth={1.5}
            >
              <path d="M250 92 L184 78" />
              <path d="M386 96 L460 90" />
              <path d="M528 118 C540 170 540 210 538 252" />
              <path d="M494 118 C430 210 372 262 340 300" />
              <path d="M318 300 L318 130" strokeDasharray="5 5" />
            </svg>
            <EdgeLabel x={186} y={64} text="org ∗FK" />
            <EdgeLabel x={398} y={70} text="[posts] 1—∞" />
            <EdgeLabel x={544} y={176} text="tags ↔ M2M" cls="text-info" />
            <EdgeLabel x={398} y={216} text="[comments]" />
            <EdgeLabel x={326} y={206} text="author ?FK" />
            {MODELS.map((m) => {
              const on = m.key === selModel;
              return (
                <button
                  key={m.key}
                  type="button"
                  onClick={() => setSelModel(m.key)}
                  style={{ left: m.x, top: m.y }}
                  className={cn(
                    "absolute w-34 rounded-[11px] border bg-card px-3 py-2.5 text-left shadow-md",
                    on
                      ? "border-primary ring-3 ring-primary/25"
                      : "border-border",
                  )}
                >
                  <div className="flex items-center gap-1.5">
                    <span className={cn("size-2 rounded-full", dotColor(m.health))} />
                    <span className="text-[14px] font-semibold">{m.key}</span>
                  </div>
                  <div className="mt-0.5 font-mono text-[11px] text-muted-foreground">
                    {m.rows}
                    {m.deadPct >= 10 ? ` · ${m.deadPct}% dead` : " rows"}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* health strip */}
        <div className="flex flex-none items-center gap-4 border-t border-border px-4 py-2 font-mono text-[12px] text-muted-foreground">
          <span className="flex items-center gap-1.5 text-ok">
            <span className="size-1.5 rounded-full bg-ok" />
            db healthy
          </span>
          <span>storage 22.4 MB</span>
          <span>dead-row 4.1%</span>
          <span className="flex items-center gap-1.5 text-warn">
            <span className="size-1.5 rounded-full bg-warn" />
            Comment · compaction reclaims 7,941 rows
          </span>
          <span className="ml-auto">last backup 2h ago</span>
        </div>
      </div>

      {/* inspector aside */}
      <aside className="flex w-[330px] flex-none flex-col border-l border-border bg-card/40">
        <div className="flex-none border-b border-border p-4">
          <div className="flex items-center gap-2">
            <span className="text-[17px] font-semibold">{sel.key}</span>
            <span className="rounded border border-border px-1.5 py-px font-mono text-[10px] text-muted-foreground">
              model
            </span>
            <span className="ml-auto font-mono text-[12px] text-muted-foreground">
              {sel.rows} rows
            </span>
          </div>
          <div className="mt-1 text-[12px] text-muted-foreground">
            {structure ? "Structure lens · at rest" : "Live lens · attached"}
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          {structure ? (
            <StructurePane sel={sel} onBrowse={() => browse({ model: sel.key })} />
          ) : connected ? (
            <LivePane
              sel={sel}
              onBrowse={() => browse({ model: sel.key })}
              onQuery={() => setScreen("console")}
              onGo={(to, label) => browse({ model: to, pivot: `${label} · from ${sel.key}` })}
            />
          ) : (
            <NotAttached
              title="Not attached"
              body="Typed records, queries and edits need the app's running API server. Structure, stats and schema still work at rest."
            />
          )}
        </div>
      </aside>
    </div>
  );
}

function StructurePane({
  sel,
  onBrowse,
}: {
  sel: (typeof MODELS)[number];
  onBrowse: () => void;
}) {
  const fields = SCHEMA[sel.key] ?? [];
  const deadWarn = sel.deadPct >= 10;
  return (
    <div>
      <div className="mb-2 text-[11px] uppercase tracking-wider text-muted-foreground">
        Fields
      </div>
      <div className="flex flex-col text-[12.5px]">
        {fields.map((f) => (
          <div
            key={f.name}
            className="flex items-baseline gap-2 border-b border-border/60 py-1.5"
          >
            <span className="min-w-24 font-mono">{f.name}</span>
            <span className="font-mono text-info">
              {f.typeLabel}
              {f.mods.length ? ` ${f.mods.join(" ")}` : ""}
            </span>
            {f.directive ? (
              <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                {f.directive}
              </span>
            ) : null}
          </div>
        ))}
      </div>

      <div className="mt-4 mb-2 text-[11px] uppercase tracking-wider text-muted-foreground">
        Storage · at rest
      </div>
      <div className="flex flex-col gap-2 font-mono text-[12.5px]">
        <Row k="data / offset" v={`${sel.dataMB} / ${sel.offMB} MB`} />
        <div>
          <div className="mb-1 flex justify-between">
            <span className="text-muted-foreground">dead rows</span>
            <span className={deadWarn ? "text-warn" : "text-ok"}>
              {sel.deadCount} · {sel.deadPct}%
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded bg-muted">
            <div
              className={cn("h-full", deadWarn ? "bg-warn" : "bg-ok")}
              style={{ width: `${sel.deadPct}%` }}
            />
          </div>
        </div>
        <Row k="compaction reclaims" v={`${sel.reclaim} rows`} />
      </div>

      <div className="mt-4 flex gap-2">
        <Button variant="outline" size="sm">
          Raw column dump
        </Button>
        <Button size="sm" onClick={onBrowse}>
          Browse rows →
        </Button>
      </div>
    </div>
  );
}

function LivePane({
  sel,
  onBrowse,
  onQuery,
  onGo,
}: {
  sel: (typeof MODELS)[number];
  onBrowse: () => void;
  onQuery: () => void;
  onGo: (to: string, label: string) => void;
}) {
  const rels = REL[sel.key] ?? [];
  return (
    <div>
      <div className="mb-3.5 grid grid-cols-2 gap-2">
        <div className="rounded-[9px] border border-border bg-card px-3 py-2.5">
          <div className="font-mono text-[11px] text-muted-foreground">rows</div>
          <div className="mt-0.5 text-[19px] font-semibold">{sel.rows}</div>
        </div>
        <div className="rounded-[9px] border border-border bg-card px-3 py-2.5">
          <div className="font-mono text-[11px] text-muted-foreground">
            indexed fields
          </div>
          <div className="mt-0.5 text-[19px] font-semibold">{sel.idxCount}</div>
        </div>
      </div>
      <div className="mb-2 text-[11px] uppercase tracking-wider text-muted-foreground">
        Relations
      </div>
      <div className="mb-4 flex flex-col gap-1.5">
        {rels.map((r) => (
          <button
            key={r.label}
            type="button"
            onClick={() => onGo(r.to, r.label)}
            className="flex items-center gap-2 rounded-lg border border-border bg-card px-2.5 py-2 text-left hover:bg-muted"
          >
            <span
              className={cn(
                "rounded border px-1.5 py-px font-mono text-[10px] font-semibold",
                relColor(r.k),
              )}
            >
              {r.kind}
            </span>
            <span className="font-mono text-[12.5px]">{r.label}</span>
            <span className="ml-auto text-muted-foreground">→</span>
          </button>
        ))}
      </div>
      <div className="flex gap-2">
        <Button size="sm" onClick={onBrowse}>
          Browse rows →
        </Button>
        <Button variant="outline" size="sm" onClick={onQuery}>
          New query
        </Button>
      </div>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{k}</span>
      <span>{v}</span>
    </div>
  );
}

function EdgeLabel({
  x,
  y,
  text,
  cls,
}: {
  x: number;
  y: number;
  text: string;
  cls?: string;
}) {
  return (
    <span
      style={{ left: x, top: y }}
      className={cn(
        "absolute rounded bg-background px-1 py-px font-mono text-[10px] text-muted-foreground",
        cls,
      )}
    >
      {text}
    </span>
  );
}

function LensButton({
  active,
  onClick,
  icon: Icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: typeof Boxes;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[12.5px] font-medium transition-colors",
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="size-3.5" />
      {label}
    </button>
  );
}
