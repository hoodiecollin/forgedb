"use client";

/**
 * Atlas relation graph (#70) — the real layout that replaced the hand-placed
 * SVG mock (#67 spike outcome: @xyflow/react + @dagrejs/dagre). Nodes come from
 * the loaded schema's models, edges from its relations (both already schema-
 * derived by the Structure lens, #12). Dagre gives a left-to-right DAG layout;
 * click selects a model, double-click browses into its rows.
 */

import { useMemo } from "react";
import Dagre from "@dagrejs/dagre";
import {
  Background,
  Controls,
  Handle,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { Model, Relation } from "@/lib/inspector/types";
import { cn } from "@/lib/utils";

const NODE_W = 172;
const NODE_H = 62;

const dotColor = (h: string) =>
  h === "warn" ? "bg-warn" : h === "danger" ? "bg-danger" : "bg-ok";

/** Edge stroke by relation kind: M2M = info, has-many = ok, FK = muted. */
const edgeStroke = (k: string) =>
  k === "m2m"
    ? "var(--color-info, #38bdf8)"
    : k === "hm"
      ? "var(--color-ok, #22c55e)"
      : "color-mix(in oklab, var(--muted-foreground) 60%, transparent)";

interface ModelNodeData extends Record<string, unknown> {
  model: Model;
  selected: boolean;
}

function ModelNode({ data }: NodeProps<Node<ModelNodeData>>) {
  const m = data.model;
  return (
    <div
      style={{ width: NODE_W }}
      className={cn(
        "rounded-[11px] border bg-card px-3 py-2.5 text-left shadow-md",
        data.selected ? "border-primary ring-3 ring-primary/25" : "border-border",
      )}
    >
      <Handle type="target" position={Position.Left} className="!bg-border" />
      <div className="flex items-center gap-1.5">
        <span className={cn("size-2 rounded-full", dotColor(m.health))} />
        <span className="truncate text-[14px] font-semibold">{m.key}</span>
      </div>
      <div className="mt-0.5 font-mono text-[11px] text-muted-foreground">
        {m.rows}
        {m.deadPct >= 10 ? ` · ${m.deadPct}% dead` : " rows"}
      </div>
      <Handle type="source" position={Position.Right} className="!bg-border" />
    </div>
  );
}

const nodeTypes = { model: ModelNode };

function layout(
  models: Model[],
  rel: Record<string, Relation[]>,
  selModel: string,
): { nodes: Node<ModelNodeData>[]; edges: Edge[] } {
  const g = new Dagre.graphlib.Graph().setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", nodesep: 34, ranksep: 96 });

  const known = new Set(models.map((m) => m.key));
  for (const m of models) g.setNode(m.key, { width: NODE_W, height: NODE_H });

  const edges: Edge[] = [];
  const seen = new Set<string>();
  for (const m of models) {
    for (const r of rel[m.key] ?? []) {
      if (!known.has(r.to)) continue; // skip dangling targets
      const id = `${m.key}->${r.to}:${r.label}`;
      if (seen.has(id)) continue;
      seen.add(id);
      g.setEdge(m.key, r.to);
      edges.push({
        id,
        source: m.key,
        target: r.to,
        label: r.kind,
        animated: r.k === "m2m",
        style: { stroke: edgeStroke(r.k), strokeWidth: 1.5 },
        labelStyle: { fontSize: 10, fontFamily: "var(--font-mono)", fill: "var(--muted-foreground)" },
        labelBgStyle: { fill: "var(--background)" },
      });
    }
  }

  Dagre.layout(g);

  const nodes: Node<ModelNodeData>[] = models.map((m) => {
    const p = g.node(m.key);
    return {
      id: m.key,
      type: "model",
      position: { x: (p?.x ?? 0) - NODE_W / 2, y: (p?.y ?? 0) - NODE_H / 2 },
      data: { model: m, selected: m.key === selModel },
      // Selection/drag is nice-to-have; keep nodes draggable for manual tidy.
    };
  });
  return { nodes, edges };
}

export function RelationGraph({
  models,
  rel,
  selModel,
  onSelect,
  onOpen,
}: {
  models: Model[];
  rel: Record<string, Relation[]>;
  selModel: string;
  onSelect: (key: string) => void;
  onOpen: (key: string) => void;
}) {
  const { nodes, edges } = useMemo(
    () => layout(models, rel, selModel),
    [models, rel, selModel],
  );

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      fitView
      fitViewOptions={{ padding: 0.2 }}
      minZoom={0.2}
      maxZoom={1.75}
      proOptions={{ hideAttribution: true }}
      onNodeClick={(_, n) => onSelect(n.id)}
      onNodeDoubleClick={(_, n) => onOpen(n.id)}
      className="bg-transparent"
    >
      <Background gap={22} size={1} color="color-mix(in oklab, var(--foreground) 8%, transparent)" />
      <Controls showInteractive={false} className="!border-border !bg-card" />
    </ReactFlow>
  );
}
