/**
 * Pure transform: raw GitHub issues + milestones + releases + epic→children
 * links → the epic-primary roadmap model.
 *
 * No I/O — scripts/build-roadmap.ts fetches and hands the raw arrays here.
 *
 * The model (see the roadmap redesign):
 *   - EPICS are the top-level unit and may span releases. Children (native
 *     GitHub sub-issues) live UNDER their epic, each annotated with where it
 *     lands (shipped in vX / done-awaiting vY / open).
 *   - STANDALONE issues (no epic parent: bug fixes, one-offs) are top-level too.
 *   - `when` is milestone-driven (the release spine); `plan-next` now means
 *     "committed but not yet scheduled to a version" (distinct from a milestone).
 *   - Forward status buckets: active (scheduled / in-flight) · planned
 *     (committed, unscheduled) · labs (experiment/rfc) · ideas (idea).
 *   - Marketing/extension scopes (`website`/`vscode`) are excluded — this is the
 *     CORE roadmap, mirroring cliff.toml's changelog scope filter.
 */

import type {
  RoadmapData,
  RoadmapItem,
  EpicItem,
  IssueItem,
  ChildIssue,
  ShippedMilestone,
  Status,
} from "./roadmap";

/** Minimal shapes we read from `gh api` (extra fields ignored). */
export interface RawIssue {
  number: number;
  title: string;
  html_url: string;
  state: "open" | "closed";
  labels: { name: string }[];
  milestone: { number: number; title: string } | null;
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
/** One epic and its native sub-issues, as fetched from issues/{n}/sub_issues. */
export interface RawEpic {
  number: number;
  children: RawIssue[];
}

/**
 * Marketing / extension / packaging scopes excluded from the CORE roadmap (they
 * ship on their own cadence, never gated on a core v* release). Mirrors
 * cliff.toml's `(website|vscode|npm|pypi|winget|docker|brew|homebrew)`, in label
 * form — `website` + `vscode` are the labels that exist.
 */
const EXCLUDED_SCOPES = new Set(["website", "vscode"]);

const CORE_TAG = /^v(\d+)\.(\d+)\.(\d+)/;

function coreVersion(tag: string): [number, number, number] | null {
  const m = CORE_TAG.exec(tag);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}
function cmpVersion(a: [number, number, number], b: [number, number, number]): number {
  return a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
}
function isoDate(ts: string | null): string | null {
  return ts ? ts.slice(0, 10) : null;
}
function labelsOf(i: RawIssue): string[] {
  return i.labels.map((l) => l.name.toLowerCase());
}
function isCoreScoped(labels: string[]): boolean {
  return !labels.some((l) => EXCLUDED_SCOPES.has(l));
}
function has(labels: string[], l: string): boolean {
  return labels.includes(l);
}

export function buildRoadmap(
  issues: RawIssue[],
  milestones: RawMilestone[],
  releases: RawRelease[],
  epics: RawEpic[],
  generatedAt: string,
): RoadmapData {
  const realIssues = issues.filter((i) => !i.pull_request);

  // Published CORE releases, keyed by tag (= milestone title).
  const publishedTags = new Set<string>();
  for (const r of releases) {
    if (!r.draft && !r.prerelease && coreVersion(r.tag_name)) publishedTags.add(r.tag_name);
  }
  const isCoreMilestone = (title: string | null): boolean =>
    title != null && coreVersion(title) != null;
  const isPublishedMilestone = (title: string | null): boolean =>
    isCoreMilestone(title) && publishedTags.has(title!);

  // Latest published core release headlines the waterline.
  let latestRelease: RoadmapData["latestRelease"] = null;
  for (const r of releases) {
    if (r.draft || r.prerelease || !coreVersion(r.tag_name)) continue;
    if (!latestRelease || cmpVersion(coreVersion(r.tag_name)!, coreVersion(latestRelease.tag)!) > 0) {
      latestRelease = { tag: r.tag_name, url: r.html_url, date: isoDate(r.published_at) };
    }
  }

  const coreMilestones = milestones.filter((m) => coreVersion(m.title));

  // Release-overview cards: published core milestones, newest first.
  const releasesOut: ShippedMilestone[] = coreMilestones
    .filter((m) => publishedTags.has(m.title))
    .map((m) => {
      const rel = releases.find((r) => r.tag_name === m.title)!;
      return {
        title: m.title,
        url: m.html_url,
        releaseUrl: rel.html_url,
        date: isoDate(rel.published_at),
        closed: m.closed_issues,
      };
    })
    .sort((a, b) => cmpVersion(coreVersion(b.title)!, coreVersion(a.title)!));

  // Next release in flight: lowest core milestone with no published release.
  const nextMs =
    coreMilestones
      .filter((m) => !publishedTags.has(m.title))
      .sort((a, b) => cmpVersion(coreVersion(a.title)!, coreVersion(b.title)!))[0] ?? null;
  const nextMilestone = nextMs
    ? { title: nextMs.title, url: nextMs.html_url, done: nextMs.closed_issues, open: nextMs.open_issues }
    : null;

  const byNumber = new Map(realIssues.map((i) => [i.number, i]));
  const childrenByEpic = new Map(epics.map((e) => [e.number, e.children.filter((c) => !c.pull_request)]));
  const claimedChildren = new Set<number>();
  for (const e of epics) for (const c of e.children) claimedChildren.add(c.number);

  // ---- Build epic items ------------------------------------------------------
  const toChild = (c: RawIssue): ChildIssue => {
    const closed = c.state === "closed";
    const core = isCoreMilestone(c.milestone?.title ?? null);
    const published = isPublishedMilestone(c.milestone?.title ?? null);
    return {
      number: c.number,
      title: c.title,
      url: c.html_url,
      state: c.state,
      milestone: c.milestone?.title ?? null,
      shipped: closed && published,
      // "pending" = done, awaiting a CORE release. A non-core milestone
      // (e.g. vscode-v*) shipped on its own line, so it is neither.
      pending: closed && core && !published,
    };
  };

  function epicStatus(labels: string[], state: "open" | "closed", children: ChildIssue[]): Status {
    if (has(labels, "experiment") || has(labels, "rfc")) return "labs";
    // A closed epic, or one whose work is entirely finished, reads as active
    // "wrapping up" rather than a forward bet; closed epics surface under Shipped.
    if (has(labels, "idea") && children.length === 0) return "ideas";
    if (children.length > 0 || state === "closed") return "active";
    return "planned";
  }

  const epicItems: EpicItem[] = [];
  for (const epic of realIssues) {
    const labels = labelsOf(epic);
    if (!has(labels, "epic") || !isCoreScoped(labels)) continue;
    const rawKids = childrenByEpic.get(epic.number) ?? [];
    const children = rawKids
      .map(toChild)
      .sort((a, b) => Number(a.state === "closed") - Number(b.state === "closed") || b.number - a.number);
    epicItems.push({
      kind: "epic",
      number: epic.number,
      title: epic.title.replace(/^\[?epic\]?:?\s*/i, ""),
      url: epic.html_url,
      state: epic.state,
      labels,
      status: epicStatus(labels, epic.state, children),
      children,
      done: children.filter((c) => c.state === "closed").length,
      total: children.length,
    });
  }

  // ---- Build standalone issue items -----------------------------------------
  /** Status for a standalone issue, or null if it's backlog/history (hidden). */
  function issueStatus(i: RawIssue, labels: string[]): Status | null {
    if (i.state === "closed") {
      // Done-but-awaiting a CORE release shows in `active`; shipped/history is on
      // the changelog + release cards, and non-core (vscode-v*) closed work is
      // hidden — it shipped on its own line, not on the core roadmap.
      const t = i.milestone?.title ?? null;
      return isCoreMilestone(t) && !isPublishedMilestone(t) ? "active" : null;
    }
    if (has(labels, "experiment") || has(labels, "rfc")) return "labs";
    if (has(labels, "idea")) return "ideas";
    if (i.milestone != null) return "active"; // scheduled to a version
    if (has(labels, "plan-next")) return "planned"; // committed, unscheduled
    return null; // bare backlog (bug/tech-debt/perf/config) — rolls up, not headlined
  }

  const issueItems: IssueItem[] = [];
  for (const i of realIssues) {
    const labels = labelsOf(i);
    if (has(labels, "epic")) continue; // epics handled above
    if (claimedChildren.has(i.number)) continue; // shown under its epic
    if (!isCoreScoped(labels)) continue;
    const status = issueStatus(i, labels);
    if (!status) continue;
    const core = isCoreMilestone(i.milestone?.title ?? null);
    const published = isPublishedMilestone(i.milestone?.title ?? null);
    issueItems.push({
      kind: "issue",
      number: i.number,
      title: i.title,
      url: i.html_url,
      state: i.state,
      labels,
      milestone: i.milestone?.title ?? null,
      shipped: i.state === "closed" && published,
      pending: i.state === "closed" && core && !published,
      status,
    });
  }

  // ---- Assemble sections -----------------------------------------------------
  // Epics first within a section, then standalone; each group newest-first.
  const byNumDesc = (a: RoadmapItem, b: RoadmapItem) => b.number - a.number;
  const section = (s: Status): RoadmapItem[] => {
    const eps = epicItems.filter((e) => e.status === s && e.state === "open").sort(byNumDesc);
    const iss = issueItems.filter((i) => i.status === s).sort(byNumDesc);
    return [...eps, ...iss];
  };

  // Only shipped epics that actually have grouped children are worth a card;
  // a closed epic with no linked sub-issues renders as an empty 0/0 and is noise.
  const shippedEpics = epicItems
    .filter((e) => e.state === "closed" && e.total > 0)
    .sort(byNumDesc);

  void byNumber; // reserved for future cross-refs

  return {
    ok: true,
    generatedAt,
    latestRelease,
    nextMilestone,
    active: section("active"),
    planned: section("planned"),
    labs: section("labs"),
    ideas: section("ideas"),
    shippedEpics,
    releases: releasesOut,
  };
}
