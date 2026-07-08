# SPIKE #67 — relation-graph library research

Research for the Inspector's **Atlas** relation-graph view (issue #67; part of #63). Goal:
find components/libs to build on instead of hand-rolling force/DAG layout + pan/zoom/edge
routing. All facts verified live against npm / GitHub / Bundlephobia on **2026-07-08**.

## TL;DR recommendation

**Render with [`@xyflow/react`](https://reactflow.dev) (React Flow) + lay out with
[`@dagrejs/dagre`](https://github.com/dagrejs/dagre).** Both MIT, both actively maintained,
both the ecosystem-standard pairing. Keep **`elkjs`** as a known upgrade path for denser edge
routing / port-based FK attachment (but its license is **EPL-2.0** — legal glance before
adopting). This is an **app-level dependency**, not a published ForgeDB substrate (identity
note below).

## The deciding insight

Our regime is **rich custom nodes** (model card: name, row count, health dot, dead-row %) at
**dozens-to-low-hundreds** of nodes. That is exactly where the canvas/WebGL "large graph"
value proposition (thousands+ nodes) does **not** apply, and where **native rich-node support
is the whole game**.

- **SVG / React-DOM renderers** (React Flow) make each node an arbitrary React component —
  native, stateful, themeable with our tokens. Perfect at our scale.
- **Canvas/WebGL libraries** (Sigma, vis-network, Cytoscape) draw nodes as shapes/shaders and
  can only *fake* rich nodes via an absolutely-positioned HTML overlay synced to pan/zoom —
  extra plumbing and z-order/event edge cases, for **zero benefit** below thousands of nodes.
- The one asset worth stealing from the canvas bucket is their **layout engines** (dagre /
  elk / fcose) — which are renderer-agnostic and feed coordinates to a React-DOM renderer just
  as well.

So: **no canvas/WebGL renderer is a better fit than React Flow here.** Their custom-node story
is too weak (Sigma/vis/Cytoscape) or too heavy for what it buys (G6).

## Renderer decision — React Flow (`@xyflow/react`)

| | React Flow `@xyflow/react` | AntV G6 v5 + react ext | Reaflow |
|---|---|---|---|
| Version / recency | **12.11.2** (2026-07-06) | 5.1.1 (2026-05-08) | 5.4.1 (2025-04, stale) |
| License | **MIT** (Pro = paid *examples/support* only, no feature gate) | MIT | Apache-2.0 |
| React 19 / Tauri | ✓ (client-only in Next) | ✓ peer `≥16.8` | partial (unverified on 19) |
| Custom rich nodes | ✓✓ **native React components** | ✓ native React nodes, but via SVG/DOM path | via `foreignObject` (event-stealing pain) |
| Typed/labeled edges | ✓ custom edge types, per-type color/marker | ✓ | ✓ (hand-built) |
| Bundled layout | ✗ (pair with dagre/elk — trivial) | ✓✓ broadest built-in (dagre/force/concentric) | ✓ elkjs built in |
| Interaction | ✓ select/pan/zoom/`fitView`/minimap built in | ✓ behavior system | ✓ (w/ caveat) |
| Bundle (gzip) | **~40–50 kB** | **~390 kB** | ~100 kB+ |
| Maintenance | ~37.5k★, released days ago, company-backed | ~12.2k★, 331 open issues | ~2k★, low velocity |

**Why React Flow over G6:** G6 is the only *other* lib with first-class React/HTML nodes, but
it renders them through the same SVG/DOM path React Flow uses natively — so at our scale it
delivers *what React Flow already does*, while adding ~340 kB of bundle and a 331-issue
backlog. Choose G6 only if we later need canvas/WebGL scale (many hundreds–thousands of
models) **and** can drop rich React nodes. We don't.

## Layout decision — `@dagrejs/dagre` (primary), `elkjs` (upgrade path)

| | `@dagrejs/dagre` | `elkjs` (ELK) | `d3-dag` |
|---|---|---|---|
| Version | **3.0.0** (2026-03-22) | 0.11.1 (2026-03-03) | 1.2.2 (2026-07-05) |
| License | **MIT** | **EPL-2.0** ⚠ (weak-copyleft; legal glance) | MIT |
| Output | coords only (renderer-agnostic) | coords only | coords only |
| Cycles / M2M back-edges | ✓ (acyclic transform; back-edges reversed) | ✓✓ (breaks cycles cleanly) | ✗ **DAG-only — breaks on M2M** |
| Ports (typed FK attach points) | ✗ | ✓ (edges attach at specific node-border ports) | ✗ |
| React Flow integration | ✓✓ **the canonical, most-documented pairing** | ✓ official examples (Elkjs Tree / Multiple Handles) | community only |
| Layout quality (dense multigraph) | good | best-in-class layered | best on *pure* DAGs, N/A for us |

- **Use `@dagrejs/dagre` first** — MIT, simplest, lowest-friction React Flow pairing, handles
  our cycles/M2M without crashing. Note: the **original `dagre` package is dead** (~7yr stale,
  being deprecated on npm); always use the **`@dagrejs/dagre`** scope.
- **`elkjs` is the upgrade** if we want ports (FK edges attaching at a specific field row on
  the card) or cleaner routing on dense M2M graphs. Blocker to weigh: **EPL-2.0** license
  (fine as an unmodified npm dependency, but not MIT — worth a legal glance).
- **`d3-dag` is rejected**: strictly DAG-only, no cycle support, so M2M/bidirectional
  relations break it unless we pre-break cycles ourselves. Not worth it over dagre.

## Rejected / not-a-fit (with reasons)

- **Sigma.js (+ @react-sigma/core)** — MIT, tiny (~40 kB), best React-19 wrapper, WebGL for
  10k+ nodes. **Eliminated:** nodes are GLSL shader programs; rich HTML cards require a fully
  DIY overlay. Its whole value (huge graphs) is irrelevant at our scale.
- **vis-network** — dual Apache-2.0/MIT, good built-in hierarchical + physics layouts. **Weak
  fit:** no maintained React 19 wrapper (mount manually via ref), canvas-only nodes, HTML
  cards are a manual `canvasToDOM` overlay.
- **Cytoscape.js (+ react-cytoscapejs)** — MIT, **best-in-class layout ecosystem**
  (dagre/fcose/elk/cola) and graph-theory API. **Overlay-based nodes:** rich cards only via
  the `cytoscape-node-html-label` overlay extension; React wrapper is stale (2022, trivially
  replaceable). Its layouts are worth remembering, but not the renderer for us.
- **react-force-graph / force-graph** — MIT, `react:*` peer (no 19 conflict), strong built-in
  force layout. **Wrong shape:** canvas/WebGL node painting (`nodeCanvasObject`) — a node
  cannot be a React card; no layered/DAG layout.
- **visx (`@visx/network`)** — MIT, best React-19 story, smallest footprint. **Renderer only,
  no layout** (README: "Does not currently handle network layout") and SVG-only nodes with
  DIY interaction — more work than React Flow for less.
- **Reaflow / Reagraph** — Apache-2.0. Reaflow: `foreignObject` event-stealing pain + stale.
  Reagraph: WebGL/three.js, ~376 kB, weak rich-card story. Neither beats React Flow here.
- **Beautiful React Diagrams** — abandoned since 2020, React ≤17. Rule out.

## Recommended starter stack

```
@xyflow/react    ^12.11        # renderer — nodes are React cards
@dagrejs/dagre   ^3.0          # layout — DAG/hierarchical coords
# elkjs ^0.11  — add later IF we need ports / dense routing (EPL-2.0, legal glance first)
```

Integration shape (well-trodden):
1. Model → React Flow `node` (custom `nodeTypes` card: name, row count, health dot, dead-row %).
2. Relation → React Flow `edge` with a **custom edge type per relation kind** — `*FK` / `?FK` /
   `[Model]` has-many / `↔ M2M` — each with its own color + arrowhead from the Inspector status
   palette. M2M gets a curved/parallel edge.
3. On load, run `dagre.layout()` over the graph, map coords → node `position`, `fitView()`.
4. Interactions come free: node select, pan/zoom, fit. Edge-follow / pivot = an `onNodeClick` /
   `onEdgeClick` handler wired to the existing pivot logic in the prototype.
5. Client-only in Next (`'use client'` + `next/dynamic` `ssr:false`); renders fine in the Tauri
   Chromium webview. All candidates share this SSR caveat — not React-Flow-specific.

## Identity note (PM guardrail)

This is **dev/ops tooling** (like the CLI): the graph lib is an **app-level dependency, not a
published ForgeDB substrate**. The generated app feeds React Flow the **already-generated**
model/relation structure; the lib never reads a `.forge` schema at runtime. Green.

## Open decision (defer to implementation)

Whether the graph becomes a **thin `RelationGraph` component in InspectorKit** (so the design
agent can compose it too) or stays an **app-only dependency**. Recommendation: start app-only;
promote a wrapper into InspectorKit only if we want the design agent iterating on the graph
view directly. Either way it does not change the identity story.
