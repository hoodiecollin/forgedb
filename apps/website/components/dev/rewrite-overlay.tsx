"use client";

import { useAtom } from "jotai";
import { useCallback, useEffect, useRef, useState } from "react";
import { usePathname } from "next/navigation";
import { rewriteModeAtom, feedbackModeAtom } from "@/lib/dev/rewrite-atoms";
import {
  detectTargets,
  detectContentTargets,
  stampedBlock,
  contentBlock,
  type DetectedTarget,
} from "@/lib/dev/rewrite-target";
import type { FeedbackMode, RewriteProposal } from "@/lib/dev/rewrite-types";
const API = "/api/dev-rewrite/";
const PRESETS = ["Tighten", "Simplify", "More precise", "Fix grammar", "Add an example"];
function slugFromPath(pathname: string): string[] | null {
  const parts = pathname.replace(/^\/+|\/+$/g, "").split("/").filter(Boolean);
  if (parts[0] !== "docs") return null;
  return parts.slice(1);
}
function moduleForPath(pathname: string): string | null {
  return pathname.replace(/\/+$/, "") === "" ? "landing" : null;
}
const insideUI = (n: EventTarget | null) =>
  n instanceof Element && n.closest("[data-rewrite-ui]") !== null;
type Phase = "idle" | "picking" | "pending" | "review";
export function RewriteOverlay() {
  const pathname = usePathname();
  const slug = slugFromPath(pathname);
  const contentModule = moduleForPath(pathname);
  const [mode, setMode] = useAtom(rewriteModeAtom);
  const [defaultFeedback] = useAtom(feedbackModeAtom);

  const [phase, setPhase] = useState<Phase>("idle");
  const [targets, setTargets] = useState<DetectedTarget[]>([]);
  const [targetIndex, setTargetIndex] = useState(0);
  const [instruction, setInstruction] = useState("");
  const [feedback, setFeedback] = useState<FeedbackMode>(defaultFeedback);
  const [requestId, setRequestId] = useState<string | null>(null);
  const [proposal, setProposal] = useState<RewriteProposal | null>(null);
  const [hoverRect, setHoverRect] = useState<DOMRect | null>(null);
  const [staleMsg, setStaleMsg] = useState<string | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const reset = useCallback(() => {
    setPhase("idle");
    setTargets([]);
    setTargetIndex(0);
    setInstruction("");
    setRequestId(null);
    setProposal(null);
    setHoverRect(null);
    window.getSelection()?.removeAllRanges();
  }, []);
  const target = targets[targetIndex] ?? null;
  const editable = slug !== null || contentModule !== null;
  const enabled = mode && editable;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey && (e.key === "e" || e.key === "E")) {
        e.preventDefault();
        setMode((m) => !m);
      } else if (e.key === "Escape" && mode) {
        if (phase === "idle") setMode(false);
        else reset();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mode, phase, reset, setMode]);
  useEffect(() => {
    if (!enabled) return;
    const onMouseUp = (e: MouseEvent) => {
      if (insideUI(e.target) || phase === "pending" || phase === "review") return;
      const found = contentModule
        ? detectContentTargets(e.target as Node)
        : detectTargets(e.target as Node);
      if (found.length) {
        setStaleMsg(null);
        setTargets(found);
        setTargetIndex(0);
        setFeedback(defaultFeedback);
        setPhase("picking");
        setTimeout(() => inputRef.current?.focus(), 0);
      }
    };
    const onClickCapture = (e: MouseEvent) => {
      if (insideUI(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
    };
    const onMouseMove = (e: MouseEvent) => {
      if (phase !== "idle" || insideUI(e.target)) {
        setHoverRect(null);
        return;
      }
      const block = (contentModule ? contentBlock : stampedBlock)(e.target as Node);
      setHoverRect(block ? block.getBoundingClientRect() : null);
    };
    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("click", onClickCapture, true);
    document.addEventListener("mousemove", onMouseMove);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("click", onClickCapture, true);
      document.removeEventListener("mousemove", onMouseMove);
    };
  }, [enabled, phase, defaultFeedback, contentModule]);
  useEffect(() => {
    if (phase !== "pending" || !requestId) return;
    let live = true;
    const tick = async () => {
      try {
        const res = await fetch(API, { cache: "no-store" });
        const { proposals } = (await res.json()) as { proposals: RewriteProposal[] };
        const mine = proposals.find((p) => p.id === requestId);
        if (mine && live) {
          setProposal(mine);
          setPhase("review");
        }
      } catch {
      }
    };
    const iv = setInterval(tick, 1000);
    tick();
    return () => {
      live = false;
      clearInterval(iv);
    };
  }, [phase, requestId]);
  if (!mode) {
    return editable ? <Fab active={false} onClick={() => setMode(true)} /> : null;
  }
  if (!editable) return null;
  async function submit() {
    if (!target || !instruction.trim()) return;
    const body =
      target.contentKey && contentModule
        ? {
            action: "request",
            contentModule,
            contentKey: target.contentKey,
            instruction: instruction.trim(),
            mode: feedback,
            target: { kind: target.kind, renderedText: target.renderedText.slice(0, 2000) },
          }
        : {
            action: "request",
            slug,
            instruction: instruction.trim(),
            mode: feedback,
            docHash:
              document
                .querySelector("[data-rewrite-doc-hash]")
                ?.getAttribute("data-rewrite-doc-hash") ?? undefined,
            target: {
              kind: target.kind,
              srcStart: target.srcStart,
              srcEnd: target.srcEnd,
              selectedText: target.selectedText,
              renderedText: target.renderedText.slice(0, 2000),
            },
          };
    const res = await fetch(API, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (res.status === 409) {
      setStaleMsg("This page changed on disk since it loaded, so the offsets are stale.");
      setPhase("idle");
      return;
    }
    const { id } = (await res.json()) as { id?: string };
    if (id) {
      setRequestId(id);
      setPhase("pending");
    }
  }
  async function decide(action: "accept" | "reject", index = 0) {
    if (!requestId) return;
    const res = await fetch(API, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ action, id: requestId, index }),
    });
    if (action === "accept" && res.status === 409) {
      setStaleMsg("This page changed on disk since it loaded, so the rewrite was not applied.");
      reset();
      return;
    }
    if (action === "accept") {
      window.location.reload();
      return;
    }
    reset();
  }
  return (
    <>
      { }
      {phase === "idle" && hoverRect && (
        <div
          className="pointer-events-none fixed z-[9998] rounded-sm ring-2 ring-primary/60 bg-primary/5"
          style={{
            left: hoverRect.left - 3,
            top: hoverRect.top - 3,
            width: hoverRect.width + 6,
            height: hoverRect.height + 6,
          }}
        />
      )}
      { }
      {phase === "picking" && target && (
        <div
          className="pointer-events-none fixed z-[9998] rounded-sm ring-2 ring-primary bg-primary/10"
          style={{
            left: target.rect.left - 3,
            top: target.rect.top - 3,
            width: target.rect.width + 6,
            height: target.rect.height + 6,
          }}
        />
      )}
      <Banner phase={phase} />
      <Fab active onClick={() => setMode(false)} />
      {phase === "picking" && target && (
        <InstructionPopover
          rect={target.rect}
          targets={targets}
          targetIndex={targetIndex}
          onKind={setTargetIndex}
          instruction={instruction}
          setInstruction={setInstruction}
          feedback={feedback}
          setFeedback={setFeedback}
          inputRef={inputRef}
          onSubmit={submit}
          onCancel={reset}
        />
      )}
      {phase === "pending" && <PendingCard onCancel={() => decide("reject")} />}
      {staleMsg && <StaleCard message={staleMsg} onDismiss={() => setStaleMsg(null)} />}
      {phase === "review" && proposal && (
        <ReviewPanel
          proposal={proposal}
          onAccept={(i) => decide("accept", i)}
          onReject={() => decide("reject")}
        />
      )}
    </>
  );
}

function Fab({ active, onClick }: { active: boolean; onClick: () => void }) {
  return (
    <button
      data-rewrite-ui
      onClick={onClick}
      title="Rewrite mode (⌥E)"
      className={`fixed bottom-4 right-4 z-[9999] flex h-11 items-center gap-2 rounded-full px-4 text-sm font-medium shadow-lg ring-1 transition ${
        active
          ? "bg-primary text-primary-foreground ring-primary"
          : "bg-background text-foreground ring-border hover:ring-primary/60"
      }`}
    >
      <span className="text-base leading-none">✎</span>
      {active ? "Rewriting — ⌥E to exit" : "Rewrite"}
    </button>
  );
}
function Banner({ phase }: { phase: Phase }) {
  const text =
    phase === "idle"
      ? "Click a block · click a heading for its section · drag-select for a span"
      : phase === "picking"
        ? "Describe the rewrite, then ⌘↵ to send"
        : phase === "pending"
          ? "Waiting for Claude to propose a rewrite…"
          : "Review the proposal below";
  return (
    <div
      data-rewrite-ui
      className="fixed left-1/2 top-3 z-[9999] -translate-x-1/2 rounded-full bg-primary px-4 py-1.5 text-xs font-medium text-primary-foreground shadow-lg"
    >
      {text}
    </div>
  );
}
const KIND_LABEL: Record<string, string> = {
  section: "Section",
  block: "Block",
  span: "Span",
  content: "Copy",
};
function popoverPos(rect: DOMRect): { left: number; top: number } {
  const width = 360;
  const left = Math.min(Math.max(8, rect.left), window.innerWidth - width - 8);
  const top = Math.min(rect.bottom + 8, window.innerHeight - 260);
  return { left, top };
}
function InstructionPopover(props: {
  rect: DOMRect;
  targets: DetectedTarget[];
  targetIndex: number;
  onKind: (i: number) => void;
  instruction: string;
  setInstruction: (s: string) => void;
  feedback: FeedbackMode;
  setFeedback: (m: FeedbackMode) => void;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  onSubmit: () => void;
  onCancel: () => void;
}) {
  const { left, top } = popoverPos(props.rect);
  const target = props.targets[props.targetIndex];
  if (!target) return null;
  return (
    <div
      data-rewrite-ui
      className="fixed z-[9999] w-[360px] rounded-lg border border-border bg-background p-3 shadow-xl"
      style={{ left, top }}
    >
      { }
      {props.targets.length > 1 && (
        <div className="mb-2 flex gap-1">
          {props.targets.map((t, i) => (
            <button
              key={t.kind}
              onClick={() => props.onKind(i)}
              className={`rounded px-2 py-0.5 text-xs font-medium ${
                i === props.targetIndex
                  ? "bg-primary text-primary-foreground"
                  : "bg-muted text-muted-foreground hover:bg-muted/70"
              }`}
            >
              {KIND_LABEL[t.kind]}
            </button>
          ))}
        </div>
      )}
      <p className="mb-2 line-clamp-2 text-xs text-muted-foreground">
        <span className="font-mono text-[10px] uppercase tracking-wide text-primary">
          {KIND_LABEL[target.kind]}
        </span>{" "}
        “{target.renderedText.slice(0, 120)}
        {target.renderedText.length > 120 ? "…" : ""}”
      </p>
      <textarea
        ref={props.inputRef}
        value={props.instruction}
        onChange={(e) => props.setInstruction(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            props.onSubmit();
          }
        }}
        placeholder="How should this read? e.g. tighten to two sentences, drop the hedging…"
        rows={3}
        className="w-full resize-none rounded border border-border bg-background px-2 py-1.5 text-sm outline-none focus:ring-1 focus:ring-primary"
      />
      <div className="mt-2 flex flex-wrap gap-1">
        {PRESETS.map((p) => (
          <button
            key={p}
            onClick={() => props.setInstruction(p)}
            className="rounded-full border border-border px-2 py-0.5 text-[11px] text-muted-foreground hover:border-primary/60 hover:text-foreground"
          >
            {p}
          </button>
        ))}
      </div>

      <div className="mt-3 flex items-center justify-between">
        <div className="flex gap-1">
          {(["diff", "candidates"] as FeedbackMode[]).map((m) => (
            <button
              key={m}
              onClick={() => props.setFeedback(m)}
              className={`rounded px-2 py-0.5 text-xs ${
                props.feedback === m
                  ? "bg-primary text-primary-foreground"
                  : "bg-muted text-muted-foreground hover:bg-muted/70"
              }`}
            >
              {m === "diff" ? "Diff" : "3 options"}
            </button>
          ))}
        </div>
        <div className="flex gap-1">
          <button
            onClick={props.onCancel}
            className="rounded px-2 py-1 text-xs text-muted-foreground hover:text-foreground"
          >
            Cancel
          </button>
          <button
            onClick={props.onSubmit}
            disabled={!props.instruction.trim()}
            className="rounded bg-primary px-3 py-1 text-xs font-medium text-primary-foreground disabled:opacity-40"
          >
            Send ⌘↵
          </button>
        </div>
      </div>
    </div>
  );
}
function PendingCard({ onCancel }: { onCancel: () => void }) {
  return (
    <div
      data-rewrite-ui
      className="fixed bottom-20 right-4 z-[9999] flex items-center gap-3 rounded-lg border border-border bg-background px-4 py-3 shadow-xl"
    >
      <span className="h-3 w-3 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      <span className="text-sm text-muted-foreground">Claude is drafting…</span>
      <button onClick={onCancel} className="text-xs text-muted-foreground hover:text-foreground">
        Cancel
      </button>
    </div>
  );
}
function StaleCard({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  return (
    <div
      data-rewrite-ui
      className="fixed bottom-20 left-1/2 z-[9999] flex -translate-x-1/2 items-center gap-3 rounded-lg border border-amber-500/40 bg-background px-4 py-3 shadow-xl"
    >
      <span className="text-amber-500">⚠</span>
      <span className="text-sm text-foreground">{message}</span>
      <button
        onClick={() => window.location.reload()}
        className="rounded bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
      >
        Reload
      </button>
      <button onClick={onDismiss} className="text-xs text-muted-foreground hover:text-foreground">
        Dismiss
      </button>
    </div>
  );
}
function ReviewPanel({
  proposal,
  onAccept,
  onReject,
}: {
  proposal: RewriteProposal;
  onAccept: (i: number) => void;
  onReject: () => void;
}) {
  const multi = proposal.mode === "candidates" && proposal.candidates.length > 1;
  const first = proposal.candidates[0];
  if (!first) return null;
  return (
    <div
      data-rewrite-ui
      className="fixed bottom-4 left-1/2 z-[9999] max-h-[70vh] w-[min(760px,92vw)] -translate-x-1/2 overflow-auto rounded-lg border border-border bg-background p-4 shadow-2xl"
    >
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-sm font-semibold">
          {multi ? "Pick a rewrite" : "Proposed rewrite"}
        </h3>
        <button
          onClick={onReject}
          className="rounded px-2 py-1 text-xs text-muted-foreground hover:text-foreground"
        >
          Reject all
        </button>
      </div>
      {!multi && <DiffView original={proposal.original} next={first.text} />}
      <div className="mt-3 space-y-3">
        {(multi ? proposal.candidates : proposal.candidates.slice(0, 1)).map((c, i) => (
          <div key={i} className="rounded border border-border">
            {multi && (
              <div className="border-b border-border px-3 py-1.5 text-xs text-muted-foreground">
                Option {i + 1}
                {c.note ? ` — ${c.note}` : ""}
              </div>
            )}
            {multi && (
              <pre className="max-h-48 overflow-auto whitespace-pre-wrap px-3 py-2 text-xs">
                {c.text}
              </pre>
            )}
            <div className="flex justify-end gap-2 border-t border-border px-3 py-2">
              <button
                onClick={() => onAccept(i)}
                className="rounded bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
              >
                Accept{multi ? ` option ${i + 1}` : ""}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
function DiffView({ original, next }: { original: string; next: string }) {
  return (
    <div className="grid gap-2 md:grid-cols-2">
      <div className="rounded border border-red-500/30 bg-red-500/5">
        <div className="border-b border-red-500/20 px-2 py-1 text-[10px] uppercase tracking-wide text-red-500/80">
          Before
        </div>
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap px-2 py-1.5 text-xs">
          {original}
        </pre>
      </div>
      <div className="rounded border border-green-500/30 bg-green-500/5">
        <div className="border-b border-green-500/20 px-2 py-1 text-[10px] uppercase tracking-wide text-green-500/80">
          After
        </div>
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap px-2 py-1.5 text-xs">
          {next}
        </pre>
      </div>
    </div>
  );
}
