import fs from "node:fs";
import path from "node:path";

export interface ChangelogRelease {
  version: string;
  unreleased: boolean;
  date: string | null;
  html: string;
  count: number;
}

const DATA_PATH = path.join(process.cwd(), "public", "changelog.json");

export function getReleases(): ChangelogRelease[] {
  try {
    const parsed = JSON.parse(fs.readFileSync(DATA_PATH, "utf8")) as { releases?: ChangelogRelease[] };
    return parsed.releases ?? [];
  } catch {
    return [];
  }
}
export function releaseAnchor(r: Pick<ChangelogRelease, "version" | "unreleased">): string {
  return r.unreleased ? "unreleased" : `v${r.version.replace(/\./g, "-")}`;
}
