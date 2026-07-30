/**
 * Pure transform: raw GitHub issues + milestones + releases → the roadmap model.
 *
 * No `gh`, no fetch, no fs — scripts/build-roadmap.ts does the I/O and hands the
 * raw arrays here. Keeping the bucket logic pure makes the waterline rules the
 * single source of truth (and trivially testable).
 *
 * The rules (see issue #210 + the 5-bucket roadmap model):
 *   - Only the CORE release line counts. Milestones/releases whose tag is not
 *     `v<major>.<minor>.<patch>` (e.g. the `vscode-v*` extension line) are dropped.
 *   - "Published" milestone = one whose title matches a non-draft, non-prerelease
 *     core GitHub Release. Its issues are SHIPPED.
 *   - A core milestone with no matching release yet is PENDING RELEASE; its closed
 *     issues are done-but-not-tagged.
 *   - Open issues bucket by label taxonomy (first match wins):
 *       experiment|rfc → Labs,  plan-next|epic → Next,  idea → Ideas.
 *     Everything else (bare bug/tech-debt/perf/config/ci) is backlog — not headlined.
 */

/** Minimal shapes we read from `gh api` (extra fields ignored). */
export interface RawIssue {
  number: number;
  title: string;
  html_url: string;
  state: "open" | "closed";
  labels: { name: string }[];
  milestone: { number: number; title: string } | null;
  /** Present on PRs; used to exclude them (the issues endpoint returns both). */
  pull_request?: unknown;
}
export interface RawMilestone {
  number: number;
  title: string;
  state: "open" | "closed";
  html_url: string;
  open_issues: number;
  closed_issues: number;
}
export interface RawRelease {
  tag_name: string;
  html_url: string;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
}

import type {
  RoadmapData,
  RoadmapIssue,
  ShippedMilestone,
  PendingRelease,
} from "./roadmap";

/**
 * Marketing / extension / packaging scopes the roadmap excludes, so it stays
 * "where the ForgeDB CORE is headed." These ship on their own cadence (the site
 * deploys continuously; the extension on the `vscode-v*` line) — never gated on a
 * core `v*` release. This mirrors cliff.toml's changelog scope filter
 * `(website|vscode|npm|pypi|winget|docker|brew|homebrew)`, expressed as issue
 * LABELS: `website` + `vscode` are the two that exist (packaging work carries no
 * label today). An issue with any of these labels is dropped from every bucket.
 */
const EXCLUDED_SCOPES = new Set(["website", "vscode"]);

function isCoreScoped(labels: string[]): boolean {
  return !labels.some((l) => EXCLUDED_SCOPES.has(l));
}

/** A core release tag: `vX.Y.Z` (optionally with a suffix we still treat as core). */
const CORE_TAG = /^v(\d+)\.(\d+)\.(\d+)/;

/** Parse a core tag into a comparable [major, minor, patch] triple, or null. */
function coreVersion(tag: string): [number, number, number] | null {
  const m = CORE_TAG.exec(tag);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

function cmpVersion(a: [number, number, number], b: [number, number, number]): number {
  return a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
}

/** "2026-07-30T16:45:47Z" → "2026-07-30" (or null). */
function isoDate(ts: string | null): string | null {
  return ts ? ts.slice(0, 10) : null;
}

function toIssue(i: RawIssue): RoadmapIssue {
  return {
    number: i.number,
    title: i.title,
    url: i.html_url,
    labels: i.labels.map((l) => l.name.toLowerCase()),
  };
}

/** Forward-bucket an open issue by its label taxonomy; null = backlog (hidden). */
export function bucketOf(labels: string[]): "labs" | "next" | "ideas" | null {
  const has = (l: string) => labels.includes(l);
  if (has("experiment") || has("rfc")) return "labs";
  if (has("plan-next") || has("epic")) return "next";
  if (has("idea")) return "ideas";
  return null;
}

export function buildRoadmap(
  issues: RawIssue[],
  milestones: RawMilestone[],
  releases: RawRelease[],
  generatedAt: string,
): RoadmapData {
  // Real issues only — the `issues` endpoint also returns PRs.
  const realIssues = issues.filter((i) => !i.pull_request);

  // Published CORE releases, keyed by their milestone-equivalent title (the tag).
  const publishedByTag = new Map<string, RawRelease>();
  for (const r of releases) {
    if (r.draft || r.prerelease || !coreVersion(r.tag_name)) continue;
    publishedByTag.set(r.tag_name, r);
  }

  // Latest published core release headlines the waterline.
  let latestRelease: RoadmapData["latestRelease"] = null;
  for (const r of publishedByTag.values()) {
    const v = coreVersion(r.tag_name)!;
    if (!latestRelease || cmpVersion(v, coreVersion(latestRelease.tag)!) > 0) {
      latestRelease = { tag: r.tag_name, url: r.html_url, date: isoDate(r.published_at) };
    }
  }

  // Core milestones only (drop the vscode-v* extension line).
  const coreMilestones = milestones.filter((m) => coreVersion(m.title));

  // Shipped: core milestones whose title has a matching published release.
  const shipped: ShippedMilestone[] = coreMilestones
    .filter((m) => publishedByTag.has(m.title))
    .map((m) => {
      const rel = publishedByTag.get(m.title)!;
      return {
        title: m.title,
        url: m.html_url,
        releaseUrl: rel.html_url,
        date: isoDate(rel.published_at),
        closed: m.closed_issues,
      };
    })
    .sort((a, b) => cmpVersion(coreVersion(b.title)!, coreVersion(a.title)!));

  // Pending release: the lowest core milestone with no matching release yet
  // (there is normally exactly one — the next release in flight).
  const pendingMilestones = coreMilestones
    .filter((m) => !publishedByTag.has(m.title))
    .sort((a, b) => cmpVersion(coreVersion(a.title)!, coreVersion(b.title)!));
  const pendingMs = pendingMilestones[0] ?? null;

  let pendingRelease: PendingRelease | null = null;
  if (pendingMs) {
    const done = realIssues
      .filter((i) => i.state === "closed" && i.milestone?.number === pendingMs.number)
      .map(toIssue)
      .filter((i) => isCoreScoped(i.labels))
      .sort((a, b) => b.number - a.number);
    pendingRelease = {
      milestone: pendingMs.title,
      url: pendingMs.html_url,
      done,
      openCount: pendingMs.open_issues,
    };
  }

  // Forward buckets from open issues.
  const next: RoadmapIssue[] = [];
  const labs: RoadmapIssue[] = [];
  const ideas: RoadmapIssue[] = [];
  for (const i of realIssues) {
    if (i.state !== "open") continue;
    const labels = i.labels.map((l) => l.name.toLowerCase());
    if (!isCoreScoped(labels)) continue; // drop website/extension/packaging scopes
    const bucket = bucketOf(labels);
    if (bucket === "next") next.push(toIssue(i));
    else if (bucket === "labs") labs.push(toIssue(i));
    else if (bucket === "ideas") ideas.push(toIssue(i));
  }
  const byNumberDesc = (a: RoadmapIssue, b: RoadmapIssue) => b.number - a.number;
  next.sort(byNumberDesc);
  labs.sort(byNumberDesc);
  ideas.sort(byNumberDesc);

  return {
    ok: true,
    generatedAt,
    latestRelease,
    pendingRelease,
    buckets: { next, labs, ideas },
    shipped,
  };
}
