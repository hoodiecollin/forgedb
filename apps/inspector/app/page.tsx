"use client";

import { useAtomValue } from "jotai";
import { screenAtom } from "@/lib/inspector/atoms";
import { useLiveTicker } from "@/lib/inspector/use-live-ticker";
import { TopBar } from "@/components/inspector/top-bar";
import { AtlasScreen } from "@/components/inspector/atlas-screen";
import { StudioScreen } from "@/components/inspector/studio-screen";
import { ConsoleScreen } from "@/components/inspector/console-screen";
import { DashboardsScreen } from "@/components/inspector/dashboards-screen";
import { RecordEditor } from "@/components/inspector/record-editor";

/**
 * The unified inspector shell: a persistent top bar + the active workflow
 * screen (Atlas / Studio / Console / Dashboards) + the record-editor drawer.
 * v1 scope = this four-screen shell (see #63).
 */
export default function InspectorPage() {
  const screen = useAtomValue(screenAtom);
  useLiveTicker();

  return (
    <div className="dark flex h-screen flex-col overflow-hidden bg-background text-foreground">
      <TopBar />
      <main className="relative min-h-0 flex-1">
        {screen === "atlas" ? <AtlasScreen /> : null}
        {screen === "studio" ? <StudioScreen /> : null}
        {screen === "console" ? <ConsoleScreen /> : null}
        {screen === "dashboards" ? <DashboardsScreen /> : null}
      </main>
      <RecordEditor />
    </div>
  );
}
