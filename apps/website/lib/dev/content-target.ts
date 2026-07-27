/**
 * Server-only resolver for the rewrite tool's *content-key* targets — the
 * key-based coordinate system for editing marketing copy that lives in a typed
 * content module (see `content/landing.ts`), as opposed to the offset-based
 * coordinate system used for `.mdx` docs (see `remark-source-map.ts`).
 *
 * A rendered element on the landing page carries `data-content-key="hero.heading"`
 * (stamped by `components/markdown.tsx`). The overlay sends that key; this module
 * parses the content module with the TypeScript compiler API, walks the key path
 * to the backing string/template literal, and returns its **inner** character
 * range in the file. The existing splice machinery (`acceptProposal`) then treats
 * `content/landing.ts` like any other file in `content/`.
 *
 * Key-based targeting is why the landing page didn't need the offset source-map:
 * a key resolves against the current file regardless of edits elsewhere, so there
 * is no offset drift — the whole-file staleness hash only guards the
 * request→accept window.
 *
 * Uses `node:fs` + the `typescript` devDependency at runtime; safe because this
 * only ever runs under the gitignored dev route (never in the export build).
 */
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { appendRequest, contentHashOfFile } from "./rewrite-queue";
import { writeBrief } from "./rewrite-brief";
import type { FeedbackMode, RewriteRequest } from "./rewrite-types";

const CONTENT_DIR = path.join(process.cwd(), "content");

/**
 * Content modules the tool can edit, by id. One page → one module for now; the
 * overlay maps a pathname to an id (see `moduleForPath`). Add a route here to
 * make another content module editable.
 */
const MODULES: Record<string, string> = {
  landing: path.join(CONTENT_DIR, "landing.ts"),
};

/** Absolute path of a content module by id, or null if unknown. */
export function contentFileFor(moduleId: string): string | null {
  return MODULES[moduleId] ?? null;
}

export interface ContentRange {
  /** Char offset (raw file) of the first char inside the literal's delimiters. */
  srcStart: number;
  /** Char offset (raw file) just past the last char inside the delimiters. */
  srcEnd: number;
  /** The current source between [srcStart, srcEnd] — the "before" text. */
  text: string;
}

/** Strip `satisfies` / `as` / parentheses wrappers to reach the real expression. */
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

/** The first exported `const` whose initializer is an object literal. */
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

/** Walk a dotted key path (object props + numeric array indices) to its value node. */
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

/** A value node → the string/template literal to splice into (or null if unsupported). */
function toLiteral(node: ts.Expression): ts.StringLiteral | ts.NoSubstitutionTemplateLiteral | null {
  let n: ts.Node | undefined = unwrap(node);
  if (n && ts.isTaggedTemplateExpression(n)) n = n.template; // dd`...`
  if (n && (ts.isNoSubstitutionTemplateLiteral(n) || ts.isStringLiteral(n))) return n;
  // A TemplateExpression (contains `${}`) has no single verbatim slice — unsupported.
  return null;
}

/**
 * Resolve `moduleId` + dotted `keyPath` to the inner char range of its backing
 * literal. Throws on an unknown module; returns null if the key doesn't resolve
 * to an editable string/template literal.
 */
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

  // Inner range: skip the opening/closing delimiter (backtick or quote).
  const srcStart = lit.getStart(sf) + 1;
  const srcEnd = lit.getEnd() - 1;
  return { srcStart, srcEnd, text: raw.slice(srcStart, srcEnd) };
}

/** Input the overlay sends for a content-key rewrite (offsets resolved here). */
export interface ContentRequestInput {
  contentModule: string;
  contentKey: string;
  instruction: string;
  mode: FeedbackMode;
  /** The rendered text the user clicked, for context; falls back to the source. */
  renderedText?: string;
}

/**
 * Resolve a content-key target, enqueue the request, and write its brief — the
 * committed core of the local dev route's content branch, so the durable logic
 * survives a fresh checkout even though the route file is gitignored. Throws on
 * unknown module / unresolvable key / empty instruction.
 */
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
      // Whole-file fingerprint: for a .ts module the body is the raw file, so this
      // guards the request→accept window against any edit to the module.
      docHash: contentHashOfFile(file),
      purpose: "marketing",
    },
    ts_now,
  );

  writeBrief(stored, file);
  return stored;
}
