/**
 * The style loader for the prose-rewrite dev tool. Composes the shared spine with
 * exactly one register file for the tier being rewritten — the deterministic
 * replacement for the old "remember to read STYLE.md" convention.
 *
 * Register ⟂ structure: the tier picks the *voice* (terse/deeper/technical); a
 * page's Build-B/Build-C structure is orthogonal (see docMetaForSlug). A Build-C
 * page reuses `terse` for its terse-native body and `technical` for its detailed
 * body, so no extra register files are needed.
 *
 * Server-only (node:fs). Style lives at `content/style/*.md`.
 */
import fs from "node:fs";
import path from "node:path";

/** The three registers, low → high on the 1–10 technical scale. */
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

/**
 * The full style guidance for a tier: the shared spine, then the tier's register.
 * This is the text that must be in front of the generator for every rewrite.
 */
export function composeStyle(tier: Tier): string {
  const read = (f: string) => fs.readFileSync(path.join(STYLE_DIR, f), "utf8").trimEnd();
  return `${read(SPINE)}\n\n---\n\n${read(REGISTER[tier])}\n`;
}
