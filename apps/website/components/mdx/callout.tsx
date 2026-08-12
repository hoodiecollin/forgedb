import type { ReactNode } from "react";
import { Info, TriangleAlert, Lightbulb, CircleCheck } from "lucide-react";
import { cn } from "@/lib/utils";

type CalloutStyle = { icon: typeof Info; cls: string; iconCls: string };
type CalloutType = "note" | "warning" | "tip" | "success";

const styles: Record<CalloutType, CalloutStyle> = {
  note: { icon: Info, cls: "border-info/30 bg-info/10", iconCls: "text-info" },
  warning: {
    icon: TriangleAlert,
    cls: "border-warn/30 bg-warn/10",
    iconCls: "text-warn",
  },
  tip: { icon: Lightbulb, cls: "border-primary/30 bg-primary/10", iconCls: "text-primary" },
  success: { icon: CircleCheck, cls: "border-ok/30 bg-ok/10", iconCls: "text-ok" },
};

// Widened past `CalloutType` on purpose. Every real call site is in an `.mdx`
// content file, where props are plain JSX attributes that TypeScript never sees
// — so `string` is the honest type of what actually arrives, and narrowing it
// here would only buy false confidence. An unrecognized name (`type="info"`,
// shipped in modifiers.mdx) used to destructure `undefined` and crash the whole
// prerender, taking the production deploy down over a one-word content typo.
// A docs typo must degrade, not fail the build.
const lookup = styles as Record<string, CalloutStyle | undefined>;

export function Callout({
  type = "note",
  title,
  children,
}: {
  type?: string;
  title?: string;
  children: ReactNode;
}) {
  const { icon: Icon, cls, iconCls } = lookup[type] ?? styles.note;
  return (
    <div className={cn("my-5 flex gap-3 rounded-lg border p-4 text-sm", cls)}>
      <Icon className={cn("mt-0.5 size-4.5 shrink-0", iconCls)} />
      <div className="min-w-0 [&>:first-child]:mt-0 [&>:last-child]:mb-0 [&_p]:my-2">
        {title ? <p className="font-medium text-foreground">{title}</p> : null}
        {children}
      </div>
    </div>
  );
}
