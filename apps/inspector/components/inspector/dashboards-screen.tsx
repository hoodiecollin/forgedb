"use client";

/**
 * Dashboards — tiles backed by saved Console queries, live tails, and at-rest
 * health. An inspector-level construct (it composes generated queries
 * client-side), not a ForgeDB engine feature — badges mark LIVE vs AT-REST vs
 * BACKUP sources.
 */

import { useAtomValue, useSetAtom } from "jotai";
import { toast } from "sonner";
import { RefreshCw } from "lucide-react";
import {
  consoleTabAtom,
  screenAtom,
  streamAtom,
} from "@/lib/inspector/atoms";
import { MODELS, POST_STATUS, SNAPS } from "@/lib/inspector/mock";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const tagColor = (k: string) =>
  k === "Added" ? "bg-ok" : k === "Updated" ? "bg-info" : "bg-danger";

export function DashboardsScreen() {
  const stream = useAtomValue(streamAtom);
  const setScreen = useSetAtom(screenAtom);
  const setConsoleTab = useSetAtom(consoleTabAtom);

  const goQuery = () => {
    setScreen("console");
    setConsoleTab("q1");
    toast("Editing widget query");
  };
  const goLive = () => {
    setScreen("console");
    setConsoleTab("live");
    toast("Editing live tail");
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-none items-center gap-2.5 border-b border-border px-5 py-3.5">
        <span className="text-[16px] font-semibold">Dashboards</span>
        <span className="text-[12.5px] text-muted-foreground">
          tiles backed by saved Console queries · live &amp; at-rest
        </span>
        <Button
          variant="outline"
          size="sm"
          className="ml-auto"
          onClick={() => setScreen("console")}
        >
          + Add widget
        </Button>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[repeat(auto-fill,minmax(300px,1fr))] content-start gap-4 overflow-auto p-5">
        {/* active users metric */}
        <Tile>
          <TileHead title="Active users · 30d" badge="LIVE" tone="ok" />
          <div className="my-1 text-[38px] leading-none font-bold">342</div>
          <div className="font-mono text-[11px] text-muted-foreground">
            User WHERE created_at ≥ 30d
          </div>
          <EditLink onClick={goQuery} label="Edit query →" />
        </Tile>

        {/* live comments tile */}
        <Tile>
          <div className="flex items-center gap-2">
            <span className="size-1.5 animate-pulse rounded-full bg-ok" />
            <span className="text-[13px] font-semibold">New comments</span>
            <span className="ml-auto font-mono text-[13px] font-semibold">
              {141 + stream.length}
            </span>
          </div>
          <div className="flex min-h-24 flex-col gap-1.5 py-2">
            {stream.slice(0, 4).map((ev, i) => (
              <div
                key={`${ev.id}-${i}`}
                className="flex items-center gap-2 font-mono text-[11px]"
              >
                <span className={cn("size-1.5 flex-none rounded-[2px]", tagColor(ev.kind))} />
                <span className="truncate">{ev.text}</span>
                <span className="ml-auto text-muted-foreground">{ev.ts}</span>
              </div>
            ))}
          </div>
          <EditLink onClick={goLive} label="Edit live tail →" />
        </Tile>

        {/* dead-row ratio health */}
        <Tile>
          <TileHead title="Dead-row ratio" badge="AT-REST" tone="muted" />
          <div className="flex flex-col gap-2">
            {MODELS.map((m) => {
              const warn = m.deadPct >= 10;
              return (
                <div
                  key={m.key}
                  className="flex items-center gap-2.5 font-mono text-[11.5px]"
                >
                  <span className="w-16 text-muted-foreground">{m.key}</span>
                  <div className="h-1.5 flex-1 overflow-hidden rounded bg-muted">
                    <div
                      className={cn("h-full", warn ? "bg-warn" : "bg-ok")}
                      style={{ width: `${Math.max(m.deadPct * 4, 2)}%` }}
                    />
                  </div>
                  <span
                    className={cn(
                      "w-9 text-right",
                      warn ? "text-warn" : "text-ok",
                    )}
                  >
                    {m.deadPct}%
                  </span>
                </div>
              );
            })}
          </div>
        </Tile>

        {/* posts by status */}
        <Tile>
          <TileHead title="Posts by status" badge="LIVE" tone="ok" />
          <div className="flex flex-col gap-2">
            {POST_STATUS.map((p) => (
              <div
                key={p.label}
                className="flex items-center gap-2.5 font-mono text-[11.5px]"
              >
                <span className="w-[74px] text-muted-foreground">{p.label}</span>
                <div className="h-1.5 flex-1 overflow-hidden rounded bg-muted">
                  <div
                    className={cn(
                      "h-full",
                      p.label === "published"
                        ? "bg-ok"
                        : p.label === "review"
                          ? "bg-warn"
                          : "bg-muted-foreground",
                    )}
                    style={{ width: `${p.pct}%` }}
                  />
                </div>
                <span className="w-11 text-right">{p.n}</span>
              </div>
            ))}
          </div>
        </Tile>

        {/* total storage */}
        <Tile>
          <TileHead title="Total storage" badge="AT-REST" tone="muted" />
          <div className="my-1 text-[38px] leading-none font-bold">
            22.4<span className="text-[18px] text-muted-foreground"> MB</span>
          </div>
          <div className="font-mono text-[11px] text-warn">
            compaction would reclaim 8,514 rows
          </div>
          <EditLink onClick={() => setScreen("atlas")} label="Open storage health →" />
        </Tile>

        {/* snapshots */}
        <Tile>
          <TileHead title="Recent snapshots" badge="BACKUP" tone="info" />
          {SNAPS.map((sn) => (
            <div
              key={sn.name}
              className="flex items-center gap-2.5 border-b border-border/60 py-1.5"
            >
              <RefreshCw className="size-3.5 text-info" />
              <div className="min-w-0">
                <div className="truncate text-[12.5px]">{sn.name}</div>
                <div className="font-mono text-[10.5px] text-muted-foreground">
                  {sn.time}
                </div>
              </div>
              <span className="ml-auto flex gap-1.5">
                <Button variant="outline" size="xs">
                  read
                </Button>
                <Button variant="outline" size="xs">
                  restore
                </Button>
              </span>
            </div>
          ))}
        </Tile>
      </div>
    </div>
  );
}

function Tile({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4">
      {children}
    </div>
  );
}

function TileHead({
  title,
  badge,
  tone,
}: {
  title: string;
  badge: string;
  tone: "ok" | "info" | "muted";
}) {
  const cls =
    tone === "ok"
      ? "border-ok/30 bg-ok/10 text-ok"
      : tone === "info"
        ? "border-info/30 bg-info/10 text-info"
        : "border-border text-muted-foreground";
  return (
    <div className="flex items-center gap-2">
      <span className="text-[13px] font-semibold">{title}</span>
      <span
        className={cn(
          "ml-auto rounded border px-1.5 py-px font-mono text-[9.5px] font-semibold",
          cls,
        )}
      >
        {badge}
      </span>
    </div>
  );
}

function EditLink({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mt-1.5 self-start text-[12px] text-info hover:underline"
    >
      {label}
    </button>
  );
}
