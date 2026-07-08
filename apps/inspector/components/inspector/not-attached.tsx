"use client";

import { useSetAtom } from "jotai";
import { toast } from "sonner";
import { PlugZap } from "lucide-react";
import { connectedAtom } from "@/lib/inspector/atoms";
import { Button } from "@/components/ui/button";

/**
 * Shown wherever a Live surface needs the running API server. The Structure
 * lens (schema, storage, stats) still works at rest, so the copy points there.
 */
export function NotAttached({
  title,
  body,
}: {
  title: string;
  body: string;
}) {
  const setConnected = useSetAtom(connectedAtom);
  return (
    <div className="max-w-sm rounded-xl border border-dashed border-border bg-card p-7 text-center">
      <div className="mx-auto mb-3 flex size-10 items-center justify-center rounded-[11px] bg-warn/15 text-warn">
        <PlugZap className="size-5" />
      </div>
      <div className="mb-1.5 text-[15px] font-semibold">{title}</div>
      <div className="mb-4 text-[13px] text-muted-foreground">{body}</div>
      <Button
        size="sm"
        onClick={() => {
          setConnected(true);
          toast("Attached to dev server :4000");
        }}
      >
        Attach to dev server →
      </Button>
    </div>
  );
}
