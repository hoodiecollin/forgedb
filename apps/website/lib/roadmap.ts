import fs from "node:fs";
import path from "node:path";

export type Status = "active" | "planned" | "labs" | "ideas";
export interface ChildIssue {
  number: number;
  title: string;
  url: string;
  state: "open" | "closed";
  milestone: string | null;
  shipped: boolean;
  pending: boolean;
}
export interface EpicItem {
  kind: "epic";
  number: number;
  title: string;
  url: string;
  state: "open" | "closed";
  labels: string[];
  status: Status;
  children: ChildIssue[];
  done: number;
  total: number;
}
export interface IssueItem {
  kind: "issue";
  number: number;
  title: string;
  url: string;
  state: "open" | "closed";
  labels: string[];
  milestone: string | null;
  shipped: boolean;
  pending: boolean;
  status: Status;
}
export type RoadmapItem = EpicItem | IssueItem;
export interface ShippedMilestone {
  title: string;
  url: string;
  releaseUrl: string;
  date: string | null;
  closed: number;
}
export interface RoadmapData {
  ok: boolean;
  generatedAt: string;
  latestRelease: { tag: string; url: string; date: string | null } | null;
  nextMilestone: { title: string; url: string; done: number; open: number } | null;
  active: RoadmapItem[];
  planned: RoadmapItem[];
  labs: RoadmapItem[];
  ideas: RoadmapItem[];
  shippedEpics: EpicItem[];
  releases: ShippedMilestone[];
}
const DATA_PATH = path.join(process.cwd(), "public", "roadmap.json");
const EMPTY: RoadmapData = {
  ok: false,
  generatedAt: "",
  latestRelease: null,
  nextMilestone: null,
  active: [],
  planned: [],
  labs: [],
  ideas: [],
  shippedEpics: [],
  releases: [],
};
export function getRoadmap(): RoadmapData {
  try {
    const parsed = JSON.parse(fs.readFileSync(DATA_PATH, "utf8")) as Partial<RoadmapData>;
    return { ...EMPTY, ...parsed };
  } catch {
    return EMPTY;
  }
}
export const GH_PROJECT_URL = "https://github.com/users/hoodiecollin/projects/3";
