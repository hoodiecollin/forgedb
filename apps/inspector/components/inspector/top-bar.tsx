"use client";

/**
 * Top bar: brand + database switcher, the workflow nav (Explore: Atlas/Studio ·
 * Query: Console/Dashboards), a snapshot "as of" affordance, and the
 * attach/detach connection toggle that gates every Live surface.
 */

import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { useEffect } from "react";
import { toast } from "sonner";
import {
  ChevronDown,
  Clock,
  Database,
  FolderOpen,
  LayoutDashboard,
  Network,
  Table2,
  TerminalSquare,
} from "lucide-react";
import {
  apiBaseAtom,
  connectedAtom,
  dbNameAtom,
  openProjectAtom,
  projectErrorAtom,
  screenAtom,
} from "@/lib/inspector/atoms";
import { isTauri } from "@/lib/inspector/data-source";
import type { Screen } from "@/lib/inspector/types";
import { cn } from "@/lib/utils";

const NAV: { screen: Screen; label: string; icon: typeof Network }[] = [
  { screen: "atlas", label: "Atlas", icon: Network },
  { screen: "studio", label: "Studio", icon: Table2 },
  { screen: "console", label: "Console", icon: TerminalSquare },
  { screen: "dashboards", label: "Dashboards", icon: LayoutDashboard },
];

export function TopBar() {
  const [screen, setScreen] = useAtom(screenAtom);
  const [connected, setConnected] = useAtom(connectedAtom);
  const dbName = useAtomValue(dbNameAtom);
  const apiBase = useAtomValue(apiBaseAtom);
  const openProject = useSetAtom(openProjectAtom);
  const [projectError, setProjectError] = useAtom(projectErrorAtom);
  const desktop = isTauri();

  // Surface an open-project failure (parse/read error) as a toast.
  useEffect(() => {
    if (projectError) {
      toast.error(projectError);
      setProjectError(null);
    }
  }, [projectError, setProjectError]);

  return (
    <header className="flex h-13 flex-none items-center gap-3.5 border-b border-border bg-card/60 px-3.5">
      {/* brand + db switcher */}
      <div className="flex items-center gap-2.5">
        <div className="flex size-6.5 items-center justify-center rounded-[7px] bg-primary text-primary-foreground">
          <Database className="size-[15px]" />
        </div>
        <div className="font-semibold tracking-tight">ForgeDB&nbsp;Inspector</div>
        <button
          type="button"
          onClick={desktop ? () => openProject() : undefined}
          title={desktop ? "Open a .forge project" : undefined}
          className="flex items-center gap-1.5 rounded-[7px] border border-border bg-muted px-2.5 py-1 text-[12.5px] hover:bg-muted/70"
        >
          <span
            className={cn(
              "size-1.5 rounded-full",
              connected ? "bg-ok" : "bg-danger",
            )}
          />
          <span className="font-mono">{dbName}</span>
          {desktop ? (
            <FolderOpen className="size-3 text-muted-foreground" />
          ) : (
            <ChevronDown className="size-3 text-muted-foreground" />
          )}
        </button>
      </div>

      {/* workflow nav */}
      <nav className="mx-auto flex items-center gap-0.5 rounded-[10px] border border-border bg-muted p-0.5">
        <span className="px-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
          Explore
        </span>
        {NAV.slice(0, 2).map((n) => (
          <NavButton key={n.screen} {...n} active={screen === n.screen} onClick={() => setScreen(n.screen)} />
        ))}
        <span className="mx-1 h-5 w-px bg-border" />
        <span className="px-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
          Query
        </span>
        {NAV.slice(2).map((n) => (
          <NavButton key={n.screen} {...n} active={screen === n.screen} onClick={() => setScreen(n.screen)} />
        ))}
      </nav>

      {/* right: snapshot + connection */}
      <div className="flex items-center gap-2.5">
        <button
          type="button"
          className="flex items-center gap-1.5 rounded-[7px] border border-dashed border-border px-2.5 py-1 text-[12px] text-muted-foreground hover:bg-muted"
        >
          <Clock className="size-3.5" />
          as of: <span className="font-mono text-foreground">now</span>
          <ChevronDown className="size-3" />
        </button>
        <button
          type="button"
          title="Toggle attached database"
          onClick={() => {
            setConnected(!connected);
            toast(connected ? "Detached from API" : `Attached to ${apiBase}`);
          }}
          className={cn(
            "flex items-center gap-1.5 rounded-[7px] border px-2.5 py-1 text-[12.5px]",
            connected
              ? "border-ok/35 bg-ok/10 text-ok"
              : "border-danger/35 bg-danger/10 text-danger",
          )}
        >
          <span
            className={cn(
              "size-1.5 rounded-full ring-3",
              connected ? "bg-ok ring-ok/30" : "bg-danger ring-danger/30",
            )}
          />
          {connected ? `attached · ${apiBase.replace(/^https?:\/\//, "")}` : "not attached"}
        </button>
      </div>
    </header>
  );
}

function NavButton({
  label,
  icon: Icon,
  active,
  onClick,
}: {
  label: string;
  icon: typeof Network;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-[7px] px-3 py-1.5 text-[13px] font-medium transition-colors",
        active
          ? "bg-card text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      <Icon className="size-3.5" />
      {label}
    </button>
  );
}
