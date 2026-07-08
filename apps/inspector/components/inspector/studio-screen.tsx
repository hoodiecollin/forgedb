"use client";

/**
 * Studio — grid-first browse/edit. Pick a model, compose predicates (bound to
 * the generated closed set, index-vs-scan flagged), see the generated request,
 * click a row to open the type-aware editor. Reverse/M2M nav pivots in place.
 */

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { toast } from "sonner";
import { Network, Plus, Search, TerminalSquare, X } from "lucide-react";
import {
  browseModelAtom,
  connectedAtom,
  liveTailAtom,
  openEditorAtom,
  pivotAtom,
  predicatesAtom,
  screenAtom,
  selectionAtom,
  studioModelAtom,
} from "@/lib/inspector/atoms";
import { GRID, MODELS } from "@/lib/inspector/mock";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { NotAttached } from "./not-attached";
import { cn } from "@/lib/utils";

const dotColor = (h: string) =>
  h === "warn" ? "bg-warn" : h === "danger" ? "bg-danger" : "bg-ok";

export function StudioScreen() {
  const connected = useAtomValue(connectedAtom);
  const [studioModel] = useAtom(studioModelAtom);
  const [pivot, setPivot] = useAtom(pivotAtom);
  const [predicates, setPredicates] = useAtom(predicatesAtom);
  const [selection, setSelection] = useAtom(selectionAtom);
  const [liveTail, setLiveTail] = useAtom(liveTailAtom);
  const browse = useSetAtom(browseModelAtom);
  const setScreen = useSetAtom(screenAtom);
  const openEditor = useSetAtom(openEditorAtom);

  const grid = GRID[studioModel] ?? GRID.User!;
  const model = MODELS.find((m) => m.key === studioModel) ?? MODELS[0]!;
  const selCount = Object.keys(selection).length;

  const scans = predicates.filter((p) => !p.idx).length;
  const req = `GET /${studioModel.toLowerCase()}s?${predicates
    .map((p) => `${p.field}${p.op === "=" ? ".eq" : ".gte"}=…`)
    .join("&")}`;

  const toggleSel = (id: string) =>
    setSelection((s) => {
      const n = { ...s };
      if (n[id]) delete n[id];
      else n[id] = true;
      return n;
    });

  const removePredicate = (i: number) =>
    setPredicates((ps) => ps.filter((_, j) => j !== i));
  const addPredicate = () =>
    setPredicates((ps) => [
      ...ps,
      { field: "bio", op: "~", val: '"eng"', idx: false },
    ]);

  return (
    <div className="flex h-full min-h-0">
      {/* model rail */}
      <aside className="flex w-52 flex-none flex-col border-r border-border bg-card/40">
        <div className="p-3">
          <div className="flex items-center gap-2 rounded-lg border border-border bg-muted px-2.5 py-1.5 text-[12.5px] text-muted-foreground">
            <Search className="size-3.5" />
            search models
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-auto px-2">
          <div className="px-1.5 pt-2 pb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
            Models · live
          </div>
          {MODELS.map((m) => (
            <button
              key={m.key}
              type="button"
              onClick={() => browse({ model: m.key })}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-[13px]",
                m.key === studioModel
                  ? "bg-primary/15 font-semibold"
                  : "hover:bg-muted",
              )}
            >
              <span className={cn("size-1.5 rounded-full", dotColor(m.health))} />
              <span>{m.key}</span>
              <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                {m.rows}
              </span>
            </button>
          ))}
          <div className="px-1.5 pt-3.5 pb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
            Structure · at rest
          </div>
          <button
            type="button"
            onClick={() => setScreen("atlas")}
            className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-[13px] text-muted-foreground hover:bg-muted"
          >
            <Network className="size-3.5" />
            Relation graph
          </button>
          <button
            type="button"
            onClick={() => setScreen("atlas")}
            className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-[13px] text-muted-foreground hover:bg-muted"
          >
            <Search className="size-3.5" />
            Storage health
          </button>
        </div>
      </aside>

      {/* grid column */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {/* title + pivot + new */}
        <div className="flex flex-none items-center gap-2.5 border-b border-border px-4 py-2.5">
          <span className="text-[16px] font-semibold">{studioModel}</span>
          {pivot ? (
            <span className="flex items-center gap-2 text-[12.5px] text-muted-foreground">
              <span>▸</span>
              <span className="rounded-md bg-info/15 px-2 py-0.5 font-mono text-info">
                {pivot}
              </span>
              <button type="button" onClick={() => setPivot(null)}>
                <X className="size-3" />
              </button>
            </span>
          ) : null}
          <span className="ml-auto" />
          <Button
            size="sm"
            onClick={() =>
              openEditor({ model: studioModel, rowId: null, mode: "create" })
            }
          >
            <Plus className="size-3.5" /> New record
          </Button>
        </div>

        {/* filter bar */}
        <div className="flex flex-none flex-wrap items-center gap-2 border-b border-border px-4 py-2.5">
          <span className="font-mono text-[11px] font-semibold text-muted-foreground">
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
              {!p.idx ? (
                <span title="not indexed — full scan">⚠</span>
              ) : null}
              <button
                type="button"
                onClick={() => removePredicate(i)}
                className="opacity-60 hover:opacity-100"
              >
                <X className="size-3" />
              </button>
            </span>
          ))}
          <button
            type="button"
            onClick={addPredicate}
            className="inline-flex items-center gap-1 rounded-md border border-dashed border-border px-2.5 py-0.5 font-mono text-[11.5px] text-muted-foreground hover:bg-muted"
          >
            + predicate
          </button>
          <span className="ml-auto flex items-center gap-2.5">
            <label className="flex cursor-pointer items-center gap-2 font-mono text-[12px] text-muted-foreground">
              <Switch checked={liveTail} onCheckedChange={setLiveTail} />
              live tail
            </label>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setScreen("console");
                toast("Filter opened in Console");
              }}
            >
              <TerminalSquare className="size-3.5" /> Open in Console
            </Button>
          </span>
        </div>

        {/* generated request line */}
        <div className="flex flex-none items-center gap-2 overflow-hidden border-b border-border px-4 py-1.5 font-mono text-[11px] text-muted-foreground">
          <span className="truncate">{req}</span>
          <span
            className={cn(
              "ml-auto whitespace-nowrap",
              scans ? "text-warn" : "text-ok",
            )}
          >
            {scans
              ? `${scans} scan · ${predicates.length - scans} indexed`
              : `all ${predicates.length} predicates indexed · fast`}
          </span>
        </div>

        {/* body: not-connected overlay OR grid */}
        {!connected ? (
          <div className="flex flex-1 items-center justify-center p-10">
            <NotAttached
              title="Attach to browse typed rows"
              body="The row grid reads typed records from the running API server. Schema and storage stats are available now in Atlas."
            />
          </div>
        ) : (
          <>
            <div className="min-h-0 flex-1 overflow-auto">
              <Table>
                <TableHeader className="sticky top-0 z-10 bg-card/80">
                  <TableRow>
                    <TableHead className="w-8" />
                    {grid.cols.map((c) => (
                      <TableHead
                        key={c.k}
                        className="font-mono text-[11px] whitespace-nowrap"
                      >
                        {c.l}
                      </TableHead>
                    ))}
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {grid.rows.map((r) => {
                    const id = r._id as string;
                    const selected = !!selection[id];
                    return (
                      <TableRow
                        key={id}
                        onClick={() =>
                          openEditor({
                            model: studioModel,
                            rowId: id,
                            mode: "edit",
                          })
                        }
                        className={cn(
                          "cursor-pointer",
                          selected && "bg-primary/10",
                        )}
                      >
                        <TableCell>
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              toggleSel(id);
                            }}
                            className={cn(
                              "flex size-3.5 items-center justify-center rounded-[4px] border text-[10px]",
                              selected
                                ? "border-primary bg-primary text-primary-foreground"
                                : "border-border",
                            )}
                          >
                            {selected ? "✓" : ""}
                          </button>
                        </TableCell>
                        {grid.cols.map((c) => {
                          const v = r[c.k];
                          const isNull = v == null;
                          return (
                            <TableCell
                              key={c.k}
                              className={cn(
                                "whitespace-nowrap",
                                c.mono && "font-mono text-[12px]",
                                isNull
                                  ? "text-muted-foreground italic"
                                  : c.rel && "text-info",
                              )}
                            >
                              {isNull ? "null" : v}
                            </TableCell>
                          );
                        })}
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </div>

            {/* footer */}
            <div className="flex flex-none items-center gap-3 border-t border-border px-4 py-2 text-[12px] text-muted-foreground">
              {selCount > 0 ? (
                <span className="flex items-center gap-2.5">
                  <span className="flex items-center gap-1.5 text-info">
                    <span className="size-1.5 rounded-full bg-info" />
                    {selCount} selected
                  </span>
                  <button type="button" className="font-semibold text-danger">
                    Delete
                  </button>
                  <button type="button" className="text-foreground">
                    Duplicate
                  </button>
                  <button type="button" className="text-foreground">
                    Export
                  </button>
                </span>
              ) : null}
              <span className="ml-auto font-mono">
                1–{grid.rows.length} of {model.rows}
              </span>
              <div className="flex gap-1">
                <Button variant="outline" size="icon-sm">
                  ◀
                </Button>
                <Button variant="outline" size="icon-sm">
                  ▶
                </Button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
