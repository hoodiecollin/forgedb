import type {
  RoadmapData,
  RoadmapItem,
  EpicItem,
  IssueItem,
  ChildIssue,
  ShippedMilestone,
  Status,
} from "./roadmap";
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
export interface RawEpic {
  number: number;
  children: RawIssue[];
}
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
  const publishedTags = new Set<string>();
  for (const r of releases) {
    if (!r.draft && !r.prerelease && coreVersion(r.tag_name)) publishedTags.add(r.tag_name);
  }
  const isCoreMilestone = (title: string | null): boolean =>
    title != null && coreVersion(title) != null;
  const isPublishedMilestone = (title: string | null): boolean =>
    isCoreMilestone(title) && publishedTags.has(title!);
  let latestRelease: RoadmapData["latestRelease"] = null;
  for (const r of releases) {
    if (r.draft || r.prerelease || !coreVersion(r.tag_name)) continue;
    if (!latestRelease || cmpVersion(coreVersion(r.tag_name)!, coreVersion(latestRelease.tag)!) > 0) {
      latestRelease = { tag: r.tag_name, url: r.html_url, date: isoDate(r.published_at) };
    }
  }
  const coreMilestones = milestones.filter((m) => coreVersion(m.title));
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

      pending: closed && core && !published,
    };
  };
  function epicStatus(labels: string[], state: "open" | "closed", children: ChildIssue[]): Status {
    if (has(labels, "experiment")) return "labs";
    if (children.length === 0 && state === "open") return "ideas";
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
  function issueStatus(i: RawIssue, labels: string[]): Status | null {
    if (i.state === "closed") {
      const t = i.milestone?.title ?? null;
      return isCoreMilestone(t) && !isPublishedMilestone(t) ? "active" : null;
    }
    if (has(labels, "experiment")) return "labs";

    const t = i.milestone?.title ?? null;
    if (t == null) return "ideas";
    return t === nextMs?.title ? "active" : "planned";
  }
  const issueItems: IssueItem[] = [];
  for (const i of realIssues) {
    const labels = labelsOf(i);
    if (has(labels, "epic")) continue;
    if (isGate(labels)) continue;
    if (claimedChildren.has(i.number)) continue;
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
  const byNumDesc = (a: RoadmapItem, b: RoadmapItem) => b.number - a.number;
  const section = (s: Status): RoadmapItem[] => {
    const eps = epicItems.filter((e) => e.status === s && e.state === "open").sort(byNumDesc);
    const iss = issueItems.filter((i) => i.status === s).sort(byNumDesc);
    return [...eps, ...iss];
  };
  const shippedEpics = epicItems
    .filter((e) => e.state === "closed" && e.total > 0)
    .sort(byNumDesc);
  void byNumber;
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
