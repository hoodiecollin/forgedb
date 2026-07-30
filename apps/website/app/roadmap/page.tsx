import type { Metadata } from "next";
import { ArrowUpRight, CircleDot, FlaskConical, Lightbulb, Rocket, CheckCircle2 } from "lucide-react";
import { getRoadmap, GH_PROJECT_URL, type RoadmapIssue } from "@/lib/roadmap";
import { site } from "@/lib/site";
import { cn } from "@/lib/utils";

export const dynamic = "error";

export const metadata: Metadata = {
  title: "Roadmap",
  description:
    "Where ForgeDB is headed — what shipped, what's done and awaiting a release, and what's planned, next, and speculative. Tied to real GitHub issues and milestones.",
};

const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/** Format "YYYY-MM-DD" without constructing a Date (avoids build-env TZ drift). */
function formatDate(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return iso;
  const [, y, mo, d] = m;
  return `${MONTHS[Number(mo) - 1] ?? mo} ${Number(d)}, ${y}`;
}

/** A single issue row: number → GitHub, title, and a couple of label chips. */
function IssueRow({ issue }: { issue: RoadmapIssue }) {
  // Only the taxonomy-meaningful labels earn a chip; drop the noisy ones.
  const chips = issue.labels.filter((l) =>
    ["epic", "rfc", "experiment", "perf", "config", "tech-debt"].includes(l),
  );
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
      <div className="min-w-0">
        <a
          href={issue.url}
          target="_blank"
          rel="noreferrer noopener"
          className="text-[15px] leading-snug text-foreground/90 decoration-primary/40 underline-offset-4 hover:underline"
        >
          {issue.title}
        </a>
        {chips.length > 0 && (
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
        )}
      </div>
    </li>
  );
}

/** A titled roadmap bucket with an icon, blurb, and issue list. */
function Section({
  icon: Icon,
  title,
  blurb,
  issues,
}: {
  icon: typeof Rocket;
  title: string;
  blurb: string;
  issues: RoadmapIssue[];
}) {
  if (issues.length === 0) return null;
  return (
    <section className="py-8">
      <div className="mb-1 flex items-center gap-2.5">
        <Icon className="size-5 text-primary" />
        <h2 className="text-xl font-semibold tracking-tight">{title}</h2>
        <span className="text-sm text-muted-foreground">{issues.length}</span>
      </div>
      <p className="mb-3 text-sm text-muted-foreground">{blurb}</p>
      <ul className="divide-y divide-border/40">
        {issues.map((i) => (
          <IssueRow key={i.number} issue={i} />
        ))}
      </ul>
    </section>
  );
}

export default function RoadmapPage() {
  const data = getRoadmap();
  const { next, labs, ideas } = data.buckets;

  return (
    <main className="mx-auto max-w-3xl px-4 py-14 sm:px-6">
      <header className="mb-6">
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">Roadmap</h1>
        <p className="mt-3 text-lg text-muted-foreground">
          Where the ForgeDB core is headed — the waterline between what has shipped, what is done and
          awaiting a release, and what is planned, experimental, or still an idea.
        </p>
      </header>

      {/* Caveat banner — this is a build-time snapshot; the GH Project is live. */}
      <div className="mb-10 rounded-lg border border-border/60 bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
        {data.ok ? (
          <>
            Snapshot as of{" "}
            <span className="font-medium text-foreground">
              {data.generatedAt ? formatDate(data.generatedAt) : "the last site build"}
            </span>
            , built from live GitHub issues and milestones. For the moment-to-moment view, see the{" "}
            <a
              href={GH_PROJECT_URL}
              target="_blank"
              rel="noreferrer noopener"
              className="font-medium text-primary underline-offset-4 hover:underline"
            >
              ForgeDB Roadmap project
              <ArrowUpRight className="ml-0.5 inline size-3.5 align-text-top" />
            </a>
            .
          </>
        ) : (
          <>
            Live roadmap data couldn&apos;t be loaded at build time. For the current plan, see the{" "}
            <a
              href={GH_PROJECT_URL}
              target="_blank"
              rel="noreferrer noopener"
              className="font-medium text-primary underline-offset-4 hover:underline"
            >
              ForgeDB Roadmap project
              <ArrowUpRight className="ml-0.5 inline size-3.5 align-text-top" />
            </a>{" "}
            and the{" "}
            <a
              href={`${site.github}/milestones`}
              target="_blank"
              rel="noreferrer noopener"
              className="font-medium text-primary underline-offset-4 hover:underline"
            >
              GitHub milestones
            </a>
            .
          </>
        )}
      </div>

      {/* Waterline: the latest shipped release + what's done awaiting the next tag. */}
      {data.latestRelease && (
        <div className="mb-4 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
          <span className="text-muted-foreground">Latest release</span>
          <a
            href={data.latestRelease.url}
            target="_blank"
            rel="noreferrer noopener"
            className="inline-flex items-center gap-1.5 rounded-full border border-primary/30 bg-primary/5 px-2.5 py-0.5 font-mono text-sm font-semibold text-primary"
          >
            <CheckCircle2 className="size-3.5" />
            {data.latestRelease.tag}
          </a>
          {data.latestRelease.date && (
            <time className="text-muted-foreground">{formatDate(data.latestRelease.date)}</time>
          )}
          <a
            href="/changelog/"
            className="text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
          >
            See the changelog →
          </a>
        </div>
      )}

      {/* Pending release — closed, awaiting the next tag (not on the changelog yet). */}
      {data.pendingRelease && data.pendingRelease.done.length > 0 && (
        <section className="rounded-lg border border-border/60 bg-gradient-to-b from-primary/[0.03] to-transparent px-5 py-6">
          <div className="mb-1 flex items-center gap-2.5">
            <Rocket className="size-5 text-primary" />
            <h2 className="text-xl font-semibold tracking-tight">
              Done — awaiting{" "}
              <a
                href={data.pendingRelease.url}
                target="_blank"
                rel="noreferrer noopener"
                className="font-mono text-primary underline-offset-4 hover:underline"
              >
                {data.pendingRelease.milestone}
              </a>
            </h2>
            <span className="text-sm text-muted-foreground">{data.pendingRelease.done.length}</span>
          </div>
          <p className="mb-3 text-sm text-muted-foreground">
            Merged and closed against the next milestone. These land on the{" "}
            <a href="/changelog/" className="underline-offset-4 hover:underline">
              changelog
            </a>{" "}
            when <span className="font-mono">{data.pendingRelease.milestone}</span> is tagged
            {data.pendingRelease.openCount > 0 && (
              <> — {data.pendingRelease.openCount} more still open in the milestone</>
            )}
            .
          </p>
          <ul className="divide-y divide-border/40">
            {data.pendingRelease.done.map((i) => (
              <IssueRow key={i.number} issue={i} />
            ))}
          </ul>
        </section>
      )}

      <Section
        icon={CircleDot}
        title="Next"
        blurb="Committed, releasable work — open epics and prioritized issues."
        issues={next}
      />
      <Section
        icon={FlaskConical}
        title="Labs"
        blurb="Experiments and RFCs — spikes to measure before they become features. Not commitments."
        issues={labs}
      />
      <Section
        icon={Lightbulb}
        title="Ideas"
        blurb="Speculative and unscheduled — directions we're considering, pending design."
        issues={ideas}
      />

      {/* Shipped — compact milestone cards; the detail lives on the changelog. */}
      {data.shipped.length > 0 && (
        <section className="mt-8 border-t border-border/60 pt-8">
          <div className="mb-3 flex items-center gap-2.5">
            <CheckCircle2 className="size-5 text-primary" />
            <h2 className="text-xl font-semibold tracking-tight">Shipped</h2>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            {data.shipped.map((m) => (
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
                  {m.date && (
                    <div className="text-xs text-muted-foreground">{formatDate(m.date)}</div>
                  )}
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
            </a>
            .
          </p>
        </section>
      )}
    </main>
  );
}
