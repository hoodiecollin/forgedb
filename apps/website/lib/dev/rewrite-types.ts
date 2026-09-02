export type TargetKind = "section" | "block" | "span" | "content";

export type Tier = "terse" | "deeper" | "technical";
export type FeedbackMode = "diff" | "candidates";
export interface RewriteTarget {
  kind: TargetKind;
  srcStart: number;
  srcEnd: number;
  selectedText: string;

  renderedText: string;
  tier?: Tier;
}
export interface RewriteRequest {
  id: string;
  ts: number;
  status: "pending" | "proposed" | "accepted" | "rejected";
  slug: string[];
  contentModule?: string;
  contentKey?: string;
  target: RewriteTarget;
  instruction: string;
  mode: FeedbackMode;
  docHash?: string;
  purpose?: "orientation" | "reference" | "marketing";
  structure?: "B" | "C";
}
export interface RewriteCandidate {
  text: string;
  note?: string;
}
export interface RewriteProposal {
  id: string;
  srcFile: string;
  srcStart: number;
  srcEnd: number;
  original: string;
  candidates: RewriteCandidate[];
  mode: FeedbackMode;
}
