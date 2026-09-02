import fs from "node:fs";
import path from "node:path";
export type Tier = "terse" | "deeper" | "technical";
const STYLE_DIR = path.join(process.cwd(), "content", "style");
const SPINE = "spine.md";
const REGISTER: Record<Tier, string> = {
  terse: "terse.md",
  deeper: "deeper.md",
  technical: "technical.md",
};

export function registerFile(tier: Tier): string {
  return REGISTER[tier];
}
export function composeStyle(tier: Tier): string {
  const read = (f: string) => fs.readFileSync(path.join(STYLE_DIR, f), "utf8").trimEnd();
  return `${read(SPINE)}\n\n---\n\n${read(REGISTER[tier])}\n`;
}
