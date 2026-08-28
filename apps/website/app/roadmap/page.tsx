import type { Metadata } from "next";
import {
  ArrowUpRight, CheckCircle2, ChevronDown, CircleDot, CircleDashed,
  FlaskConical, Lightbulb, Rocket, Layers,
} from "lucide-react";
import {
  getRoadmap, GH_PROJECT_URL,
  type RoadmapItem, type EpicItem, type IssueItem, type ChildIssue,
} from "@/lib/roadmap";
import { site } from "@/lib/site";
import { cn } from "@/lib/utils";

export const dynamic = "error";

export const metadata: Metadata = {
  title: "Roadmap",
  description:
    "Where the ForgeDB core is headed — initiatives grouped by epic, with what's in progress, planned, experimental, and speculative. Tied to real GitHub issues and milestones.",
};

const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];
function formatDate(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return iso;
  const [, y, mo, d] = m;
  return `${MONTHS[Number(mo) - 1] ?? mo} ${Number(d)}, ${y}`;
}

const CHIP_LABELS = ["experiment", "bugfix", "hotfix", "release-gate"];
function WhenChip({ item }: { item: Pick<ChildIssue, "shipped" | "pending" | "state" | "milestone"> }) {
  if (item.shipped) {
    return (
      <span className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
        <CheckCircle2 className="size-3" />
        {item.milestone}
      </span>
    );
  }
  if (item.pending) {
    return (
      <span className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
        <CheckCircle2 className="size-3" />
        done · {item.milestone}
      </span>
    );
  }
  if (item.milestone) {
    return (
      <span className="inline-flex items-center gap-1 rounded border border-primary/30 bg-primary/5 px-1.5 py-0.5 text-[11px] font-medium text-primary">
        {item.milestone}
      </span>
    );
  }
  return null;
}
function labelChips(labels: string[]) {
  const chips = labels.filter((l) => CHIP_LABELS.includes(l));
  if (chips.length === 0) return null;
  return (
    <span className="ml-2 inline-flex flex-wrap gap-1 align-middle">
      {chips.map((c) => (
        <span
          key={c}
          className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground"
        >
          {c}
        </span>
      ))}
    </span>
  );
}
function IssueRow({ issue }: { issue: IssueItem }) {
  return (
    <li className="group flex items-baseline gap-3 py-2">
      <a
        href={issue.url}
        target="_blank"
        rel="noreferrer noopener"
        className="shrink-0 font-mono text-xs text-muted-foreground transition-colors group-hover:text-primary"
      >
        #{issue.number}
      </a>
      <div className="min-w-0 flex-1">
        <a
          href={issue.url}
          target="_blank"
          rel="noreferrer noopener"
          className="text-[15px] leading-snug text-foreground/90 decoration-primary/40 underline-offset-4 hover:underline"
        >
          {issue.title}
        </a>
        {labelChips(issue.labels)}
      </div>
      <WhenChip item={issue} />
    </li>
  );
}
function ChildRow({ child }: { child: ChildIssue }) {
  const Icon = child.state === "closed" ? CheckCircle2 : CircleDashed;
  return (
    <li className="group flex items-baseline gap-2.5 py-1.5">
      <Icon
        className={cn(
          "mt-0.5 size-3.5 shrink-0",
          child.shipped ? "text-emerald-500" : child.pending ? "text-amber-500" : "text-muted-foreground/40",
        )}
      />
      <a
        href={child.url}
        target="_blank"
        rel="noreferrer noopener"
        className="shrink-0 font-mono text-xs text-muted-foreground transition-colors group-hover:text-primary"
      >
        #{child.number}
      </a>
      <span
        className={cn(
          "min-w-0 flex-1 text-sm leading-snug",
          child.state === "closed" ? "text-muted-foreground" : "text-foreground/90",
        )}
      >
        {child.title}
      </span>
      <WhenChip item={child} />
    </li>
  );
}
function EpicCard({ epic, open }: { epic: EpicItem; open?: boolean }) {
  const pct = epic.total > 0 ? Math.round((epic.done / epic.total) * 100) : 0;
  return (
    <details open={open} className="group rounded-lg border border-border/60 px-4 py-3">
      <summary className="flex cursor-pointer list-none items-center gap-3 [&::-webkit-details-marker]:hidden">
        <Layers className="size-4 shrink-0 text-primary" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <a
              href={epic.url}
              target="_blank"
              rel="noreferrer noopener"
              className="font-mono text-xs text-muted-foreground hover:text-primary"
            >
              #{epic.number}
            </a>
            <span className="truncate text-[15px] font-semibold tracking-tight">{epic.title}</span>
          </div>
          <div className="mt-1.5 flex items-center gap-2">
            <div className="h-1.5 w-24 overflow-hidden rounded-full bg-muted">
              <div className="h-full rounded-full bg-primary/70" style={{ width: `${pct}%` }} />
            </div>
            <span className="text-xs text-muted-foreground">
              {epic.done} / {epic.total} done
            </span>
          </div>
        </div>
        <ChevronDown className="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180" />
      </summary>
      <ul className="mt-3 border-t border-border/40 pt-2">
        {epic.children.map((c) => (
          <ChildRow key={c.number} child={c} />
        ))}
      </ul>
    </details>
  );
}
function ItemView({ item, openEpics }: { item: RoadmapItem; openEpics?: boolean }) {
  return item.kind === "epic" ? <EpicCard epic={item} open={openEpics} /> : <IssueRow issue={item} />;
}
function Section({
  icon: Icon, title, blurb, items, openEpics,
}: {
  icon: typeof Rocket;
  title: string;
  blurb: string;
  items: RoadmapItem[];
  openEpics?: boolean;
}) {
  if (items.length === 0) return null;
  const epics = items.filter((i) => i.kind === "epic");
  const issues = items.filter((i) => i.kind === "issue");
  return (
    <section className="py-7">
      <div className="mb-1 flex items-center gap-2.5">
        <Icon className="size-5 text-primary" />
        <h2 className="text-xl font-semibold tracking-tight">{title}</h2>
        <span className="text-sm text-muted-foreground">{items.length}</span>
      </div>
      <p className="mb-3 text-sm text-muted-foreground">{blurb}</p>
      {epics.length > 0 && (
        <div className="mb-3 space-y-2">
          {epics.map((e) => (
            <ItemView key={`e${e.number}`} item={e} openEpics={openEpics} />
          ))}
        </div>
      )}
      {issues.length > 0 && (
        <ul className="divide-y divide-border/40">
          {issues.map((i) => (
            <ItemView key={`i${i.number}`} item={i} />
          ))}
        </ul>
      )}
    </section>
  );
}
export default function RoadmapPage() {
  const data = getRoadmap();
  return (
    <main className="mx-auto max-w-3xl px-4 py-14 sm:px-6">
      <header className="mb-6">
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">Roadmap</h1>
        <p className="mt-3 text-lg text-muted-foreground">
          Where the ForgeDB core is headed — initiatives grouped under their epic, with standalone
          fixes alongside. What&apos;s in progress, planned, experimental, and still an idea.
        </p>
      </header>
      { }
      <div className="mb-8 rounded-lg border border-border/60 bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
        {data.ok ? (
          <>
            Snapshot as of{" "}
            <span className="font-medium text-foreground">
              {data.generatedAt ? formatDate(data.generatedAt) : "the last site build"}
            </span>
            , built from live GitHub issues, milestones, and epic sub-issues. For the
            moment-to-moment view, see the{" "}
            <a href={GH_PROJECT_URL} target="_blank" rel="noreferrer noopener"
              className="font-medium text-primary underline-offset-4 hover:underline">
              ForgeDB Roadmap project
              <ArrowUpRight className="ml-0.5 inline size-3.5 align-text-top" />
            </a>.
          </>
        ) : (
          <>
            Live roadmap data couldn&apos;t be loaded at build time. For the current plan, see the{" "}
            <a href={GH_PROJECT_URL} target="_blank" rel="noreferrer noopener"
              className="font-medium text-primary underline-offset-4 hover:underline">
              ForgeDB Roadmap project
              <ArrowUpRight className="ml-0.5 inline size-3.5 align-text-top" />
            </a>{" "}
            and the{" "}
            <a href={`${site.github}/milestones`} target="_blank" rel="noreferrer noopener"
              className="font-medium text-primary underline-offset-4 hover:underline">
              GitHub milestones
            </a>.
          </>
        )}
      </div>
      { }
      {(data.latestRelease || data.nextMilestone) && (
        <div className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
          {data.latestRelease && (
            <>
              <span className="text-muted-foreground">Latest</span>
              <a
                href={data.latestRelease.url}
                target="_blank"
                rel="noreferrer noopener"
                className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/5 px-2.5 py-0.5 font-mono text-sm font-semibold text-emerald-600 dark:text-emerald-400"
              >
                <CheckCircle2 className="size-3.5" />
                {data.latestRelease.tag}
              </a>
            </>
          )}
          {data.nextMilestone && (
            <>
              <span className="text-muted-foreground">· Next</span>
              <a
                href={data.nextMilestone.url}
                target="_blank"
                rel="noreferrer noopener"
                className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/5 px-2.5 py-0.5 font-mono text-sm font-semibold text-primary"
              >
                <Rocket className="size-3.5" />
                {data.nextMilestone.title}
              </a>
              <span className="text-muted-foreground">
                {data.nextMilestone.done} done · {data.nextMilestone.open} open
              </span>
            </>
          )}
          <a href="/changelog/" className="ml-auto text-muted-foreground underline-offset-4 hover:text-foreground hover:underline">
            Changelog →
          </a>
        </div>
      )}
      <Section
        icon={Rocket}
        title="In progress"
        blurb="Scheduled to a release or actively in flight. Epics show per-child progress; done-but-untagged work lands on the changelog when its release is cut."
        items={data.active}
        openEpics
      />
      <Section
        icon={CircleDot}
        title="Planned"
        blurb="Committed, but not yet scheduled to a version."
        items={data.planned}
      />
      <Section
        icon={FlaskConical}
        title="Labs"
        blurb="Experiments and RFCs — spikes to measure before they become features. Not commitments."
        items={data.labs}
      />
      <Section
        icon={Lightbulb}
        title="Ideas"
        blurb="Speculative and unscheduled — directions we're considering, pending design."
        items={data.ideas}
      />
      { }
      {(data.shippedEpics.length > 0 || data.releases.length > 0) && (
        <section className="mt-8 border-t border-border/60 pt-8">
          <div className="mb-3 flex items-center gap-2.5">
            <CheckCircle2 className="size-5 text-primary" />
            <h2 className="text-xl font-semibold tracking-tight">Shipped</h2>
          </div>
          {data.shippedEpics.length > 0 && (
            <div className="mb-4 space-y-2">
              {data.shippedEpics.map((e) => (
                <EpicCard key={e.number} epic={e} />
              ))}
            </div>
          )}
          <div className="grid gap-3 sm:grid-cols-2">
            {data.releases.map((m) => (
              <a
                key={m.title}
                href={m.releaseUrl}
                target="_blank"
                rel="noreferrer noopener"
                className={cn(
                  "flex items-center justify-between rounded-lg border border-border/60 px-4 py-3",
                  "transition-colors hover:border-primary/40 hover:bg-muted/30",
                )}
              >
                <div>
                  <div className="font-mono text-sm font-semibold">{m.title}</div>
                  {m.date && <div className="text-xs text-muted-foreground">{formatDate(m.date)}</div>}
                </div>
                <div className="text-right text-xs text-muted-foreground">
                  {m.closed} {m.closed === 1 ? "issue" : "issues"}
                  <ArrowUpRight className="ml-1 inline size-3.5 align-text-top" />
                </div>
              </a>
            ))}
          </div>
          <p className="mt-3 text-sm text-muted-foreground">
            Full per-release detail is on the{" "}
            <a href="/changelog/" className="font-medium text-primary underline-offset-4 hover:underline">
              changelog
            </a>.
          </p>
        </section>
      )}
    </main>
  );
}
