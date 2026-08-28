import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { appendRequest, contentHashOfFile } from "./rewrite-queue";
import { writeBrief } from "./rewrite-brief";
import type { FeedbackMode, RewriteRequest } from "./rewrite-types";
const CONTENT_DIR = path.join(process.cwd(), "content");
const MODULES: Record<string, string> = {
  landing: path.join(CONTENT_DIR, "landing.ts"),
};
export function contentFileFor(moduleId: string): string | null {
  return MODULES[moduleId] ?? null;
}
export interface ContentRange {
  srcStart: number;
  srcEnd: number;
  text: string;
}

function unwrap(node: ts.Expression | undefined): ts.Expression | undefined {
  let n = node;
  while (
    n &&
    (ts.isSatisfiesExpression(n) || ts.isAsExpression(n) || ts.isParenthesizedExpression(n))
  ) {
    n = n.expression;
  }
  return n;
}
function findExportedObject(sf: ts.SourceFile): ts.ObjectLiteralExpression | null {
  for (const stmt of sf.statements) {
    if (!ts.isVariableStatement(stmt)) continue;
    const exported = stmt.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword);
    if (!exported) continue;
    for (const decl of stmt.declarationList.declarations) {
      const init = unwrap(decl.initializer);
      if (init && ts.isObjectLiteralExpression(init)) return init;
    }
  }
  return null;
}
function propName(name: ts.PropertyName): string | null {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name)) return name.text;
  return null;
}
function navigate(root: ts.ObjectLiteralExpression, segments: string[]): ts.Expression | null {
  let current: ts.Expression = root;
  for (const seg of segments) {
    const node = unwrap(current);
    if (!node) return null;
    if (ts.isObjectLiteralExpression(node)) {
      const prop = node.properties.find(
        (p): p is ts.PropertyAssignment =>
          ts.isPropertyAssignment(p) && propName(p.name) === seg,
      );
      if (!prop) return null;
      current = prop.initializer;
    } else if (ts.isArrayLiteralExpression(node)) {
      const idx = Number(seg);
      const element = Number.isInteger(idx) ? node.elements[idx] : undefined;
      if (!element) return null;
      current = element;
    } else {
      return null;
    }
  }
  return current;
}
function toLiteral(node: ts.Expression): ts.StringLiteral | ts.NoSubstitutionTemplateLiteral | null {
  let n: ts.Node | undefined = unwrap(node);
  if (n && ts.isTaggedTemplateExpression(n)) n = n.template;
  if (n && (ts.isNoSubstitutionTemplateLiteral(n) || ts.isStringLiteral(n))) return n;
  return null;
}
export function resolveContentTarget(moduleId: string, keyPath: string): ContentRange | null {
  const file = contentFileFor(moduleId);
  if (!file) throw new Error(`unknown content module: ${moduleId}`);
  const raw = fs.readFileSync(file, "utf8");
  const sf = ts.createSourceFile(file, raw, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const root = findExportedObject(sf);
  if (!root) return null;
  const value = navigate(root, keyPath.split("."));
  if (!value) return null;
  const lit = toLiteral(value);
  if (!lit) return null;
  const srcStart = lit.getStart(sf) + 1;
  const srcEnd = lit.getEnd() - 1;
  return { srcStart, srcEnd, text: raw.slice(srcStart, srcEnd) };
}

export interface ContentRequestInput {
  contentModule: string;
  contentKey: string;
  instruction: string;
  mode: FeedbackMode;
  renderedText?: string;
}

export function enqueueContentRequest(input: ContentRequestInput, ts_now: number): RewriteRequest {
  const { contentModule, contentKey, instruction, mode, renderedText } = input;
  if (!contentKey || typeof instruction !== "string" || !instruction.trim()) {
    throw new Error("missing contentKey/instruction");
  }
  const file = contentFileFor(contentModule);
  if (!file) throw new Error(`unknown content module: ${contentModule}`);

  const resolved = resolveContentTarget(contentModule, contentKey);
  if (!resolved) throw new Error(`no content slot "${contentKey}" in ${contentModule}`);

  const stored = appendRequest(
    {
      slug: [],
      contentModule,
      contentKey,
      target: {
        kind: "content",
        srcStart: resolved.srcStart,
        srcEnd: resolved.srcEnd,
        selectedText: "",
        renderedText: (renderedText ?? resolved.text).slice(0, 2000),
      },
      instruction: instruction.trim(),
      mode: mode === "candidates" ? "candidates" : "diff",

      docHash: contentHashOfFile(file),
      purpose: "marketing",
    },
    ts_now,
  );
  writeBrief(stored, file);
  return stored;
}
