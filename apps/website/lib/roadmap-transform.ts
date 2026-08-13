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
 *   - `when` is milestone-driven (the release spine). Under the two-axis model a
 *     milestone means COMMITTED, and being the cycle in flight is what means
 *     SCHEDULED — so the milestone alone separates active from planned.
 *   - Forward status buckets: active (on the cycle in flight) · planned
 *     (committed to a later milestone) · labs (experiment) · ideas (no milestone).
 *   - GATE sub-issues are excluded entirely; they are process, not roadmap.
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
 * The core roadmap covers the core product surface only. Every OTHER shippable
 * surface (`surface:website`, `surface:ide-extension`, …) ships on its own
 * cadence / tag namespace and is excluded — the playbook's surface-exclusion
 * rule, in label form. Mirrors cliff.toml's commit-scope filter
 * `(website|vscode|npm|pypi|winget|docker|brew|homebrew)`.
 *
 * Matched by PREFIX rather than an enumerated set, so a new surface label is
 * excluded the moment it exists (and so the 2026-07-31 rename of the bare
 * `website` / `vscode` labels to `surface:*` can't silently unfilter them
 * again). `surface:core` is the core line itself and is never excluded.
 */
const SURFACE_PREFIX = "surface:";
const CORE_SURFACE = "surface:core";

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
  return !labels.some((l) => l.startsWith(SURFACE_PREFIX) && l !== CORE_SURFACE);
}
function has(labels: string[], l: string): boolean {
  return labels.includes(l);
}

/**
 * Gate sub-issues (`improvement:gate-1`, `bugfix:gate-2`, …) are process artifacts, not roadmap
 * entries. They are children of a WORK ITEM rather than of an epic, so the `claimedChildren` filter
 * below — which only knows about epic children — does not catch them, and without this they would
 * each render as a standalone roadmap card. A single milestone's materialized gate set is ~26
 * issues, so this is the difference between a roadmap and a task dump.
 *
 * Matched by pattern rather than an enumerated list so a new work type's gates are excluded the
 * moment the type exists.
 */
const GATE_LABEL = /^[a-z]+:gate-\d+$/;
function isGate(labels: string[]): boolean {
  return labels.some((l) => GATE_LABEL.test(l));
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

  /*
   * Next release in flight: the lowest core milestone on an UNRELEASED LINE.
   *
   * The "on an unreleased line" clause is what keeps a hotfix from hijacking the cycle. A patch
   * milestone (v0.4.1) sorts below the real cycle (v0.5.0) and has no release of its own, so
   * "lowest milestone with no published release" picks the patch — and every v0.5.0 item then
   * reads as `planned` rather than `active` for the whole hotfix window. A published v0.4.0 is the
   * evidence that the 0.4 line already shipped, so the whole line is excluded, not just that tag.
   * PLAYBOOK §5.6, and the same rule pm-playbook's own `currentCycle` applies.
   */
  const releasedLines = new Set<string>();
  for (const tag of publishedTags) {
    const v = coreVersion(tag)!;
    releasedLines.add(`${v[0]}.${v[1]}`);
  }
  const onReleasedLine = (title: string): boolean => {
    const v = coreVersion(title)!;
    return releasedLines.has(`${v[0]}.${v[1]}`);
  };

  const nextMs =
    coreMilestones
      .filter((m) => !publishedTags.has(m.title) && !onReleasedLine(m.title))
      .sort((a, b) => cmpVersion(coreVersion(a.title)!, coreVersion(b.title)!))[0] ?? null;
  const nextMilestone = nextMs
    ? { title: nextMs.title, url: nextMs.html_url, done: nextMs.closed_issues, open: nextMs.open_issues }
    : null;

  const byNumber = new Map(realIssues.map((i) => [i.number, i]));
  const childrenByEpic = new Map(
    epics.map((e) => [e.number, e.children.filter((c) => !c.pull_request && !isGate(labelsOf(c)))]),
  );
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
    if (has(labels, "experiment")) return "labs";
    // An epic with nothing under it yet is a forward bet, not work in progress. Under the two-axis
    // model there is no `idea` label to consult — an epic that has decomposed into nothing is the
    // definition of one.
    if (children.length === 0 && state === "open") return "ideas";
    // A closed epic, or one whose work is entirely finished, reads as active
    // "wrapping up" rather than a forward bet; closed epics surface under Shipped.
    return "active";
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
    // An `experiment`'s deliverable is a finding, not a shippable artifact — it never carries a
    // milestone, so it is checked before the milestone axis rather than after it.
    if (has(labels, "experiment")) return "labs";
    // The two-axis model puts the whole remaining answer on the milestone:
    //   none            → uncommitted, i.e. an idea
    //   not the cycle   → committed, not yet scheduled
    //   the cycle       → scheduled, in flight
    // There are no maturity labels left to consult; `idea` and `plan-next` were retired precisely
    // because they were a second copy of this, free to disagree with it.
    const t = i.milestone?.title ?? null;
    if (t == null) return "ideas";
    return t === nextMs?.title ? "active" : "planned";
  }

  const issueItems: IssueItem[] = [];
  for (const i of realIssues) {
    const labels = labelsOf(i);
    if (has(labels, "epic")) continue; // epics handled above
    if (isGate(labels)) continue; // process artifact, not a roadmap entry
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
