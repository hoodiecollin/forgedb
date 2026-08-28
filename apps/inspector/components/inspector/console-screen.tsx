"use client";

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { toast } from "sonner";
import { Clock, RefreshCw, Star } from "lucide-react";
import {
  connectedAtom,
  consoleTabAtom,
  pinnedSnapshotsAtom,
  pinSnapshotAtom,
  predicatesAtom,
  screenAtom,
  snapshotTokenAtom,
  streamAtom,
  studioModelAtom,
} from "@/lib/inspector/atoms";
import { isTauri } from "@/lib/inspector/data-source";
import { GRID, SAVED, SNAPS, TAILS } from "@/lib/inspector/mock";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
const CONSOLE_COLS = ["id", "email", "created_at", "org →"];
const tagColor = (k: string) =>
  k === "Added" ? "bg-ok" : k === "Updated" ? "bg-info" : "bg-danger";

export function ConsoleScreen() {
  const [tab, setTab] = useAtom(consoleTabAtom);
  const predicates = useAtomValue(predicatesAtom);
  const stream = useAtomValue(streamAtom);
  const connected = useAtomValue(connectedAtom);
  const [snapshotToken, setSnapshotToken] = useAtom(snapshotTokenAtom);
  const pinned = useAtomValue(pinnedSnapshotsAtom);
  const pinSnapshot = useSetAtom(pinSnapshotAtom);
  const setScreen = useSetAtom(screenAtom);
  const setStudioModel = useSetAtom(studioModelAtom);
  const studioModel = useAtomValue(studioModelAtom);
  const rows = (GRID.User?.rows ?? []).slice(0, 5);
  const idxCount = predicates.filter((p) => p.idx).length;
  const scanCount = predicates.length - idxCount;

  const queryCode = `query User {\n${predicates
    .map(
      (p) =>
        `  ${p.field}${p.op === "=" ? ".eq(" : ".gte("}${p.val})${p.idx ? "" : "   // scan"}`,
    )
    .join(",\n")}\n}`;
  const activePinName =
    snapshotToken &&
    pinned.find((p) => JSON.stringify(p.token) === JSON.stringify(snapshotToken))
      ?.name;
  const activeWatermark = snapshotToken ? snapshotToken[studioModel] : undefined;
  const snapReadout = !snapshotToken
    ? "now (live)"
    : `${activePinName ?? "snapshot"} · ${studioModel} @ ${activeWatermark ?? "?"} rows`;
  const pinCurrent = async () => {
    if (!isTauri() || !connected) {
      toast.error("Attach to a running API to pin a snapshot");
      return;
    }
    const name = `snapshot ${pinned.length + 1}`;
    try {
      await pinSnapshot(name);
      toast(`Pinned "${name}"`);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };
  return (
    <div className="flex h-full min-h-0">
      { }
      <aside className="flex w-56 flex-none flex-col overflow-auto border-r border-border bg-card/40">
        <RailHeading>Saved queries</RailHeading>
        {SAVED.map((q) => (
          <button
            key={q.name}
            type="button"
            onClick={() => {
              setTab("q1");
              toast(`Loaded "${q.name}"`);
            }}
            className="mx-1 flex items-center gap-2 rounded-lg px-3 py-1.5 text-left text-[13px] hover:bg-muted"
          >
            <Star className="size-3.5 text-warn" />
            {q.name}
          </button>
        ))}
        <RailHeading>Live tails</RailHeading>
        {TAILS.map((t) => (
          <button
            key={t.name}
            type="button"
            onClick={() => setTab("live")}
            className="mx-1 flex items-center gap-2 rounded-lg px-3 py-1.5 text-left text-[13px] hover:bg-muted"
          >
            <span
              className={cn(
                "size-1.5 rounded-full",
                t.k === "info" ? "bg-info" : "bg-ok",
              )}
            />
            {t.name}
          </button>
        ))}
        <RailHeading>Snapshots</RailHeading>
        {SNAPS.map((sn) => (
          <button
            key={sn.name}
            type="button"
            onClick={() => setTab("snap")}
            className="mx-1 block rounded-lg px-3 py-1.5 text-left hover:bg-muted"
          >
            <div className="flex items-center gap-2 text-[13px]">
              <RefreshCw className="size-3.5 text-info" />
              {sn.name}
            </div>
            <div className="pl-5 font-mono text-[10.5px] text-muted-foreground">
              {sn.time}
            </div>
          </button>
        ))}
      </aside>
      { }
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        { }
        <div className="flex flex-none items-center border-b border-border px-2">
          <TabBtn active={tab === "q1"} onClick={() => setTab("q1")}>
            Users where…
          </TabBtn>
          <TabBtn active={tab === "live"} onClick={() => setTab("live")}>
            <span className="size-1.5 rounded-full bg-ok" />
            Live: comments
          </TabBtn>
          <TabBtn active={tab === "snap"} onClick={() => setTab("snap")}>
            <RefreshCw className="size-3 text-info" />
            Snapshot
          </TabBtn>
        </div>
        { }
        {tab === "q1" ? (
          <div className="flex min-h-0 flex-1 flex-col overflow-auto">
            <div className="border-b border-border px-4 py-3.5">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-[12px] text-muted-foreground">
                  on
                </span>
                <span className="rounded-md bg-primary/15 px-2.5 py-0.5 font-mono text-[13px] font-semibold">
                  User
                </span>
                <span className="font-mono text-[12px] text-muted-foreground">
                  WHERE
                </span>
                {predicates.map((p, i) => (
                  <span
                    key={`${p.field}-${i}`}
                    className={cn(
                      "inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 font-mono text-[11.5px]",
                      p.idx
                        ? "border-ok/40 bg-ok/10 text-ok"
                        : "border-warn/45 bg-warn/10 text-warn",
                    )}
                  >
                    {p.field} {p.op} {p.val}
                    {!p.idx ? <span title="full scan">⚠</span> : null}
                  </span>
                ))}
              </div>
            </div>
            <div className="border-b border-border px-4 py-3.5">
              <div className="mb-2 flex items-center gap-2">
                <span className="text-[11px] uppercase tracking-wider text-muted-foreground">
                  Generated request
                </span>
                <span className="ml-auto flex gap-1.5">
                  <Button variant="outline" size="xs">
                    copy
                  </Button>
                  <Button variant="outline" size="xs">
                    save
                  </Button>
                  <Button variant="outline" size="xs">
                    share
                  </Button>
                </span>
              </div>
              <pre className="overflow-auto rounded-[9px] border border-border bg-background/60 px-3.5 py-3 font-mono text-[12.5px] leading-relaxed text-foreground/90">
                {queryCode}
              </pre>
            </div>
            <div className="flex items-center gap-3 border-b border-border px-4 py-2.5">
              <span className="font-mono text-[12px] text-muted-foreground">
                156 rows · 8 ms · {idxCount} indexed / {scanCount} scan
              </span>
              <span className="ml-auto flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setScreen("studio");
                    setStudioModel("User");
                    toast("Results opened in Studio grid");
                  }}
                >
                  Open in grid
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setScreen("dashboards");
                    toast("Saved as dashboard widget");
                  }}
                >
                  Save as widget
                </Button>
                <Button size="sm">⌗ Run</Button>
              </span>
            </div>
            <ResultsTable rows={rows} />
          </div>
        ) : null}
        { }
        {tab === "live" ? (
          <div className="flex min-h-0 flex-1 flex-col overflow-auto">
            <div className="flex items-center gap-3 border-b border-border px-4 py-3.5">
              <span className="inline-flex items-center gap-1.5 rounded-full border border-ok/35 bg-ok/10 px-2.5 py-1 font-mono text-[12.5px] text-ok">
                <span className="size-1.5 rounded-full bg-ok" />
                {connected ? "subscribed" : "offline"}
              </span>
              <span className="font-mono text-[12.5px] text-muted-foreground">
                Comment WHERE post.eq(…) · streaming deltas
              </span>
              <span className="ml-auto font-mono text-[13px] font-semibold">
                {141 + stream.length} rows
              </span>
            </div>
            <div className="flex items-center gap-4 border-b border-border px-4 py-2 font-mono text-[11.5px] text-muted-foreground">
              <Legend color="bg-ok" label="Added" />
              <Legend color="bg-info" label="Updated" />
              <Legend color="bg-danger" label="Removed" />
            </div>
            {stream.map((ev, i) => (
              <div
                key={`${ev.id}-${i}`}
                className="flex items-center gap-3 border-b border-border/60 px-4 py-2.5 font-mono text-[12.5px]"
              >
                <span
                  className={cn(
                    "min-w-14 rounded px-1.5 py-0.5 text-center text-[9px] font-bold text-primary-foreground",
                    tagColor(ev.kind),
                  )}
                >
                  {ev.kind}
                </span>
                <span>{ev.text}</span>
                <span className="ml-auto text-muted-foreground">{ev.ts}</span>
              </div>
            ))}
            <div className="flex items-center gap-2 px-4 py-3.5 text-[12.5px] text-muted-foreground">
              <span className="size-1.5 animate-pulse rounded-full bg-ok" />
              watching for changes…
            </div>
          </div>
        ) : null}
        { }
        {tab === "snap" ? (
          <div className="flex min-h-0 flex-1 flex-col overflow-auto">
            <div className="border-b border-border px-4 py-4">
              <div className="mb-3 flex items-center gap-2.5">
                <span className="text-[11px] uppercase tracking-wider text-muted-foreground">
                  Time-travel
                </span>
                <span className="ml-auto font-mono text-[13px] font-semibold">
                  reading as of <span className="text-info">{snapReadout}</span>
                </span>
              </div>
              { }
              <div className="flex flex-wrap items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => setSnapshotToken(null)}
                  className={cn(
                    "flex items-center gap-1.5 rounded-[7px] border px-2.5 py-1 font-mono text-[12px]",
                    !snapshotToken
                      ? "border-ok/40 bg-ok/10 text-ok"
                      : "border-border text-muted-foreground hover:bg-muted",
                  )}
                >
                  <span
                    className={cn(
                      "size-1.5 rounded-full",
                      !snapshotToken ? "bg-ok" : "bg-muted-foreground/40",
                    )}
                  />
                  now (live)
                </button>
                {pinned.map((p) => (
                  <button
                    key={p.name}
                    type="button"
                    onClick={() => setSnapshotToken(p.token)}
                    title={`${studioModel} @ ${p.token[studioModel] ?? "?"} rows`}
                    className={cn(
                      "flex items-center gap-1.5 rounded-[7px] border px-2.5 py-1 font-mono text-[12px]",
                      activePinName === p.name
                        ? "border-info/40 bg-info/10 text-info"
                        : "border-border text-muted-foreground hover:bg-muted",
                    )}
                  >
                    <RefreshCw className="size-3" />
                    {p.name}
                  </button>
                ))}
                <button
                  type="button"
                  onClick={pinCurrent}
                  className="ml-auto flex items-center gap-1.5 rounded-[7px] border border-dashed border-border px-2.5 py-1 text-[12px] text-muted-foreground hover:bg-muted"
                >
                  <Clock className="size-3.5" />
                  Pin current…
                </button>
              </div>
              <div className="mt-2.5 text-[12px] text-muted-foreground">
                A snapshot is a consistent point-in-time view across <b>all</b>{" "}
                models, captured as each model&rsquo;s row-count watermark — a
                read taken here shows the data exactly as it was, even after later
                changes. The Studio grid follows the selected snapshot.
              </div>
            </div>
            <div className="flex items-center gap-2.5 border-b border-border px-4 py-2.5 font-mono text-[12px] text-muted-foreground">
              {studioModel} · {snapReadout}
              <span className="ml-auto text-info">
                compare vs current → (tool builds this)
              </span>
            </div>
            <ResultsTable rows={rows} />
          </div>
        ) : null}
      </div>
    </div>
  );
}
function ResultsTable({
  rows,
}: {
  rows: Record<string, string | null>[];
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow className="bg-card/80">
          {CONSOLE_COLS.map((c) => (
            <TableHead key={c} className="font-mono text-[11px]">
              {c}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((r) => (
          <TableRow key={r._id as string}>
            <TableCell className="font-mono text-[12px]">{r.id}</TableCell>
            <TableCell
              className={cn(
                "font-mono text-[12px]",
                r.email == null && "text-muted-foreground italic",
              )}
            >
              {r.email ?? "null"}
            </TableCell>
            <TableCell className="font-mono text-[12px]">{r.created}</TableCell>
            <TableCell className="text-info">→ {r.org}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
function RailHeading({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-3 pt-3.5 pb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
      {children}
    </div>
  );
}
function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 border-b-2 px-3.5 py-2.5 text-[13px] transition-colors",
        active
          ? "border-primary font-semibold text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}
function Legend({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className={cn("size-2.5 rounded-[3px]", color)} />
      {label}
    </span>
  );
}
