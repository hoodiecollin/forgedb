"use client";

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Boxes, Activity } from "lucide-react";
import {
  browseModelAtom,
  connectedAtom,
  lensAtom,
  modelsAtom,
  projectSourceAtom,
  relAtom,
  schemaAtom,
  screenAtom,
  selModelAtom,
} from "@/lib/inspector/atoms";
import type { Model } from "@/lib/inspector/types";
import { Button } from "@/components/ui/button";
import { NotAttached } from "./not-attached";
import { RelationGraph } from "./relation-graph";
import { cn } from "@/lib/utils";
const relColor = (k: string) =>
  k === "m2m" ? "text-info border-info/50" : k === "hm" ? "text-ok border-ok/50" : "text-muted-foreground border-border";
export function AtlasScreen() {
  const [lens, setLens] = useAtom(lensAtom);
  const [selModel, setSelModel] = useAtom(selModelAtom);
  const connected = useAtomValue(connectedAtom);
  const models = useAtomValue(modelsAtom);
  const rel = useAtomValue(relAtom);
  const source = useAtomValue(projectSourceAtom);
  const browse = useSetAtom(browseModelAtom);
  const setScreen = useSetAtom(screenAtom);
  const sel = models.find((m) => m.key === selModel) ?? models[0];
  const structure = lens === "structure";

  const showMockEdges = source === "mock";
  const relCount = models.reduce((n, m) => n + (rel[m.key]?.length ?? 0), 0);
  if (!sel) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <NotAttached
          title="No models"
          body="This schema has no models to display."
        />
      </div>
    );
  }
  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col">
        { }
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
            <span className="font-mono text-[12px] text-muted-foreground">
              {models.length} models · {relCount} relations
            </span>
          </div>
        </div>
        { }
        <div className="min-h-0 flex-1">
          <RelationGraph
            models={models}
            rel={rel}
            selModel={selModel}
            onSelect={setSelModel}
            onOpen={(key) => browse({ model: key })}
          />
        </div>
        { }
        {showMockEdges ? (
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
        ) : (
          <ProjectHealthStrip models={models} />
        )}
      </div>
      { }
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
  sel: Model;
  onBrowse: () => void;
}) {
  const schema = useAtomValue(schemaAtom);
  const fields = schema[sel.key] ?? [];
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
  sel: Model;
  onBrowse: () => void;
  onQuery: () => void;
  onGo: (to: string, label: string) => void;
}) {
  const rel = useAtomValue(relAtom);
  const rels = rel[sel.key] ?? [];
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
function ProjectHealthStrip({ models }: { models: Model[] }) {
  const withStats = models.filter((m) => m.rows !== "—");
  const totalMB = withStats.reduce((s, m) => s + (parseFloat(m.dataMB) || 0), 0);
  const worst = withStats.reduce<Model | null>(
    (w, m) => (!w || m.deadPct > w.deadPct ? m : w),
    null,
  );
  return (
    <div className="flex flex-none items-center gap-4 border-t border-border px-4 py-2 font-mono text-[12px] text-muted-foreground">
      {withStats.length > 0 ? (
        <>
          <span className="flex items-center gap-1.5 text-ok">
            <span className="size-1.5 rounded-full bg-ok" />
            {withStats.length} models · at rest
          </span>
          <span>storage {totalMB.toFixed(1)} MB</span>
          {worst && worst.deadPct >= 10 ? (
            <span className="flex items-center gap-1.5 text-warn">
              <span className="size-1.5 rounded-full bg-warn" />
              {worst.key} · compaction reclaims {worst.reclaim} rows
            </span>
          ) : null}
        </>
      ) : (
        <span>schema only — open a data directory for storage stats</span>
      )}
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
