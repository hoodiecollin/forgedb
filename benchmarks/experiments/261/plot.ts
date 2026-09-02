#!/usr/bin/env bun
import { readFileSync, writeFileSync } from "node:fs";
type Rec = {
  capacity: string;
  n_chars: number;
  overflow: string;
  pct_inline: number;
  rows: number;
  value_bytes: number;
  variant: string;
  ns_total: number;
  ns_per_row: number;
  bytes_on_disk: number;
};
type Raw = {
  mixes: number[];
  capacities: { label: string; n: number }[];
  overflows: { label: string; lo: number; hi: number }[];
  control: { drift_pct: number; ns_per_row_min: number; ns_per_row_max: number };
  grid: Rec[];
  scale: { rows: number; variant: string; ns_per_row: number; ns_total: number }[];
};

const raw: Raw = JSON.parse(readFileSync("results/raw.json", "utf8"));
const SERIES = [
  { key: "p_hand", label: "pointer (idealized)", color: "#1f77b4", dash: "" },
  { key: "p_real", label: "pointer (shipping)", color: "#7fb8db", dash: "4 3" },
  { key: "i1", key2: "h1", label: "inline N+4", color: "#2ca02c", dash: "" },
  { key: "i4", key2: "h4", label: "inline 4N+4", color: "#d62728", dash: "" },
] as const;
const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
function fmtNs(v: number): string {
  if (v >= 10000) return `${(v / 1000).toFixed(0)}µs`;
  if (v >= 1000) return `${(v / 1000).toFixed(1)}µs`;
  if (v >= 100) return v.toFixed(0);
  if (v >= 10) return v.toFixed(1);
  return v.toFixed(2);
}
function fmtBytes(v: number): string {
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)}${u[i]}`;
}
const PANEL_W = 300;
const PANEL_H = 200;
const PAD_L = 62;
const PAD_R = 16;
const PAD_T = 34;
const PAD_B = 40;
const COL_GAP = 26;
const ROW_GAP = 34;
const HEAD_H = 108;
const caps = raw.capacities;
const ovfs = raw.overflows;
const mixes = raw.mixes;
const gridW =
  60 + ovfs.length * (PAD_L + PANEL_W + PAD_R) + (ovfs.length - 1) * COL_GAP;
const gridH =
  HEAD_H + caps.length * (PAD_T + PANEL_H + PAD_B) + (caps.length - 1) * ROW_GAP + 30;

function pick(cap: string, ovf: string, variant: string): Map<number, Rec> {
  const m = new Map<number, Rec>();
  for (const r of raw.grid) {
    if (r.capacity === cap && r.overflow === ovf && r.variant === variant) {
      m.set(r.pct_inline, r);
    }
  }
  return m;
}
const out: string[] = [];
out.push(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${gridW}" height="${gridH}" viewBox="0 0 ${gridW} ${gridH}" font-family="ui-sans-serif, -apple-system, Helvetica, Arial, sans-serif">`,
);
out.push(`<rect width="${gridW}" height="${gridH}" fill="#ffffff"/>`);
out.push(
  `<text x="30" y="34" font-size="21" font-weight="700" fill="#111">Experiment #261 — inline <tspan font-family="ui-monospace, monospace">string(N)</tspan> slot vs pointer indirection</text>`,
);
out.push(
  `<text x="30" y="58" font-size="13" fill="#555">Full column scan, every value read. Lower is better. Rows = declared inline capacity N; columns = how far an overflowing value exceeds N.</text>`,
);

const ctl = (raw.control as any).samples as number[];
const ctlSorted = [...ctl].sort((a, b) => a - b);
const ctlMed = ctlSorted[Math.floor(ctlSorted.length / 2)];
const ctlSteady = ctl.filter((v) => v < ctlMed * 1.15).length;
out.push(
  `<text x="30" y="76" font-size="13" fill="#555">Per-panel y-axis (ranges differ by orders of magnitude). Warm page cache, median of ${(raw as any).reps ?? 7} reps. In-run control steady in ${ctlSteady}/${ctl.length} rounds (within 15% of ${ctlMed.toFixed(2)} ns/row); the ${ctl.length - ctlSteady} excursions are all huge×massive panels, where the config under test evicts the control's pages.</text>`,
);
let lx = 30;
for (const s of SERIES) {
  out.push(
    `<line x1="${lx}" y1="94" x2="${lx + 26}" y2="94" stroke="${s.color}" stroke-width="2.5" ${s.dash ? `stroke-dasharray="${s.dash}"` : ""}/>`,
  );
  out.push(
    `<text x="${lx + 32}" y="98" font-size="12" fill="#333">${esc(s.label)}</text>`,
  );
  lx += 42 + s.label.length * 6.6;
}
out.push(
  `<circle cx="${lx + 6}" cy="94" r="4" fill="none" stroke="#111" stroke-width="1.6"/>`,
);
out.push(
  `<text x="${lx + 16}" y="98" font-size="12" fill="#333">hard form (no branch), 100% inline only</text>`,
);
for (let ri = 0; ri < caps.length; ri++) {
  for (let ci = 0; ci < ovfs.length; ci++) {
    const cap = caps[ri];
    const ovf = ovfs[ci];
    const ox = 60 + ci * (PAD_L + PANEL_W + PAD_R + COL_GAP) + PAD_L;
    const oy = HEAD_H + ri * (PAD_T + PANEL_H + PAD_B + ROW_GAP) + PAD_T;
    const series = SERIES.map((s) => ({
      s,
      data: pick(cap.label, ovf.label, s.key),
      hard: (s as any).key2 ? pick(cap.label, ovf.label, (s as any).key2) : new Map(),
    }));
    let vmax = 0;
    for (const { data, hard } of series) {
      for (const r of data.values()) vmax = Math.max(vmax, r.ns_per_row);
      for (const r of hard.values()) vmax = Math.max(vmax, r.ns_per_row);
    }
    vmax = vmax * 1.12 || 1;
    const x = (pct: number) => ox + (mixes.indexOf(pct) / (mixes.length - 1)) * PANEL_W;
    const y = (v: number) => oy + PANEL_H - (v / vmax) * PANEL_H;
    out.push(
      `<rect x="${ox}" y="${oy}" width="${PANEL_W}" height="${PANEL_H}" fill="#fafafa" stroke="#e3e3e3"/>`,
    );
    for (let g = 1; g <= 4; g++) {
      const v = (vmax / 4) * g;
      out.push(
        `<line x1="${ox}" y1="${y(v)}" x2="${ox + PANEL_W}" y2="${y(v)}" stroke="#ececec"/>`,
      );
      out.push(
        `<text x="${ox - 6}" y="${y(v) + 4}" font-size="10" fill="#888" text-anchor="end">${fmtNs(v)}</text>`,
      );
    }
    out.push(
      `<text x="${ox - 46}" y="${oy + PANEL_H / 2}" font-size="10" fill="#888" text-anchor="middle" transform="rotate(-90 ${ox - 46} ${oy + PANEL_H / 2})">ns / row</text>`,
    );
    for (const m of mixes) {
      const showLabel = [5, 20, 50, 80, 100].includes(m);
      out.push(
        `<line x1="${x(m)}" y1="${oy + PANEL_H}" x2="${x(m)}" y2="${oy + PANEL_H + (showLabel ? 5 : 3)}" stroke="#bbb"/>`,
      );
      if (showLabel) {
        out.push(
          `<text x="${x(m)}" y="${oy + PANEL_H + 17}" font-size="10" fill="#888" text-anchor="middle">${m}</text>`,
        );
      }
    }
    out.push(
      `<text x="${ox + PANEL_W / 2}" y="${oy + PANEL_H + 32}" font-size="10.5" fill="#777" text-anchor="middle">% of rows fitting inline</text>`,
    );
    out.push(
      `<text x="${ox}" y="${oy - 16}" font-size="13" font-weight="650" fill="#222">N=${cap.n} <tspan font-weight="400" fill="#666">(${esc(cap.label)})</tspan> · overflow ${esc(ovf.label)} <tspan font-weight="400" fill="#888">${ovf.lo}–${ovf.hi}×N</tspan></text>`,
    );
    for (const { s, data, hard } of series) {
      const pts = mixes.filter((m) => data.has(m)).map((m) => `${x(m)},${y(data.get(m)!.ns_per_row)}`);
      if (pts.length > 1) {
        out.push(
          `<polyline points="${pts.join(" ")}" fill="none" stroke="${s.color}" stroke-width="2.1" ${s.dash ? `stroke-dasharray="${s.dash}"` : ""} stroke-linejoin="round"/>`,
        );
      }
      for (const m of mixes) {
        const r = data.get(m);
        if (r) out.push(`<circle cx="${x(m)}" cy="${y(r.ns_per_row)}" r="2.2" fill="${s.color}"/>`);
        const h = hard.get(m);
        if (h) {
          out.push(
            `<circle cx="${x(m)}" cy="${y(h.ns_per_row)}" r="4.2" fill="none" stroke="${s.color}" stroke-width="1.8"/>`,
          );
        }
      }
    }
  }
}
out.push("</svg>");
writeFileSync("results/grid.svg", out.join("\n"));
const TIE_PCT = 2;
function crossover(cap: string, ovf: string, variant: string): number | "tie" | null {
  const v = pick(cap, ovf, variant);
  const p = pick(cap, ovf, "p_hand");
  let tied = false;
  for (const m of mixes) {
    const a = v.get(m);
    const b = p.get(m);
    if (!a || !b) continue;
    const delta = (a.ns_per_row / b.ns_per_row - 1) * 100;
    if (delta < -TIE_PCT) return m;
    if (delta <= TIE_PCT) tied = true;
  }
  return tied ? "tie" : null;
}
function amp(cap: string, ovf: string, variant: string, m: number): number | null {
  const a = pick(cap, ovf, variant).get(m);
  const b = pick(cap, ovf, "p_hand").get(m);
  if (!a || !b || b.bytes_on_disk === 0) return null;
  return a.bytes_on_disk / b.bytes_on_disk;
}
const CW = 128;
const CH = 40;
const tblW = 190 + ovfs.length * CW;
const sumW = Math.max(1180, 60 + tblW * 2 + 60);
const sumH = 190 + caps.length * CH + 330;
const s2: string[] = [];
s2.push(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${sumW}" height="${sumH}" viewBox="0 0 ${sumW} ${sumH}" font-family="ui-sans-serif, -apple-system, Helvetica, Arial, sans-serif">`,
);
s2.push(`<rect width="${sumW}" height="${sumH}" fill="#ffffff"/>`);
s2.push(
  `<text x="30" y="34" font-size="21" font-weight="700" fill="#111">Experiment #261 — summary</text>`,
);
s2.push(
  `<text x="30" y="57" font-size="13" fill="#555">Left: the lowest inline share at which the inline layout reads faster than the idealized pointer baseline. Right: bytes on disk relative to pointer storage.</text>`,
);

function drawTable(
  x0: number,
  y0: number,
  title: string,
  sub: string,
  cell: (cap: string, ovf: string) => { text: string; fill: string; fg?: string },
) {
  s2.push(`<text x="${x0}" y="${y0 - 26}" font-size="14" font-weight="650" fill="#222">${esc(title)}</text>`);
  s2.push(`<text x="${x0}" y="${y0 - 8}" font-size="11.5" fill="#777">${esc(sub)}</text>`);
  for (let ci = 0; ci < ovfs.length; ci++) {
    s2.push(
      `<text x="${x0 + 190 + ci * CW + CW / 2}" y="${y0 + 14}" font-size="11.5" font-weight="600" fill="#444" text-anchor="middle">${esc(ovfs[ci].label)}</text>`,
    );
    s2.push(
      `<text x="${x0 + 190 + ci * CW + CW / 2}" y="${y0 + 28}" font-size="10" fill="#999" text-anchor="middle">${ovfs[ci].lo}–${ovfs[ci].hi}×N</text>`,
    );
  }
  for (let ri = 0; ri < caps.length; ri++) {
    const yy = y0 + 38 + ri * CH;
    s2.push(
      `<text x="${x0 + 182}" y="${yy + CH / 2 + 4}" font-size="12" fill="#333" text-anchor="end">N=${caps[ri].n} <tspan fill="#999">${esc(caps[ri].label)}</tspan></text>`,
    );
    for (let ci = 0; ci < ovfs.length; ci++) {
      const c = cell(caps[ri].label, ovfs[ci].label);
      const xx = x0 + 190 + ci * CW;
      s2.push(
        `<rect x="${xx + 2}" y="${yy + 2}" width="${CW - 4}" height="${CH - 4}" fill="${c.fill}" stroke="#fff" stroke-width="1"/>`,
      );
      s2.push(
        `<text x="${xx + CW / 2}" y="${yy + CH / 2 + 4}" font-size="12" font-weight="600" fill="${c.fg ?? "#222"}" text-anchor="middle">${esc(c.text)}</text>`,
      );
    }
  }
}
const y0 = 120;
const crossCell = (cap: string, ovf: string, variant: string) => {
  const c = crossover(cap, ovf, variant);
  if (c === null) return { text: "never", fill: "#f6d5d3", fg: "#8b1a17" };
  if (c === "tie") return { text: "tie", fill: "#eeeeee", fg: "#555" };
  const fill = c >= 99 ? "#fbeee0" : c >= 80 ? "#fdf3d8" : "#dff0d8";
  return { text: `${c}%`, fill, fg: c > 90 ? "#7a5b13" : "#245b2b" };
};
drawTable(30, y0, "Crossover — inline 4N+4 (as #238 declares it)", `lowest % of rows inline at which it beats the pointer baseline by >${TIE_PCT}%; “never” = loses at every mix`, (cap, ovf) =>
  crossCell(cap, ovf, "i4"),
);
drawTable(30 + tblW + 60, y0, "Storage — inline 4N+4 bytes on disk ÷ pointer", "at a 50% mix; >1 means the inline layout stores more", (cap, ovf) => {
  const a = amp(cap, ovf, "i4", 50);
  if (a === null) return { text: "—", fill: "#eee" };
  const fill = a > 4 ? "#f6d5d3" : a > 2 ? "#fbeee0" : a > 1.2 ? "#fdf3d8" : "#dff0d8";
  return { text: `${a.toFixed(a < 10 ? 2 : 0)}×`, fill };
});
const y1 = y0 + 38 + caps.length * CH + 76;
drawTable(30, y1, "Crossover — inline N+4 (if N counted bytes, not chars)", "the same mechanism without the 4× worst-case-UTF-8 slot reservation", (cap, ovf) =>
  crossCell(cap, ovf, "i1"),
);

const sx = 30 + tblW + 60;
const sy = y1;
const SW = 420;
const SH = 200;
s2.push(`<text x="${sx}" y="${sy - 26}" font-size="14" font-weight="650" fill="#222">Step vs slope — N=16, overflow 3–5×N, 50% inline</text>`);
s2.push(`<text x="${sx}" y="${sy - 8}" font-size="11.5" fill="#777">flat with row count ⇒ the cost is a per-row slope, not a per-scan step that amortizes</text>`);
const scaleRows = [...new Set(raw.scale.map((r) => r.rows))].sort((a, b) => a - b);
const scaleMax = Math.max(...raw.scale.map((r) => r.ns_per_row)) * 1.12;
const sxp = (r: number) => sx + (scaleRows.indexOf(r) / (scaleRows.length - 1)) * SW;
const syp = (v: number) => sy + 30 + SH - (v / scaleMax) * SH;
s2.push(`<rect x="${sx}" y="${sy + 30}" width="${SW}" height="${SH}" fill="#fafafa" stroke="#e3e3e3"/>`);
for (let g = 1; g <= 4; g++) {
  const v = (scaleMax / 4) * g;
  s2.push(`<line x1="${sx}" y1="${syp(v)}" x2="${sx + SW}" y2="${syp(v)}" stroke="#ececec"/>`);
  s2.push(`<text x="${sx - 6}" y="${syp(v) + 4}" font-size="10" fill="#888" text-anchor="end">${fmtNs(v)}</text>`);
}
for (const r of scaleRows) {
  s2.push(`<text x="${sxp(r)}" y="${sy + 30 + SH + 16}" font-size="10" fill="#888" text-anchor="middle">${r >= 1e6 ? "1M" : r >= 1000 ? `${r / 1000}k` : r}</text>`);
}
s2.push(`<text x="${sx + SW / 2}" y="${sy + 30 + SH + 32}" font-size="10.5" fill="#777" text-anchor="middle">rows scanned</text>`);
for (const s of SERIES) {
  const pts = scaleRows
    .map((r) => raw.scale.find((v) => v.rows === r && v.variant === s.key))
    .filter(Boolean)
    .map((v) => `${sxp(v!.rows)},${syp(v!.ns_per_row)}`);
  if (pts.length > 1) {
    s2.push(
      `<polyline points="${pts.join(" ")}" fill="none" stroke="${s.color}" stroke-width="2.1" ${s.dash ? `stroke-dasharray="${s.dash}"` : ""}/>`,
    );
  }
}
let lx2 = sx;
for (const s of SERIES) {
  s2.push(`<line x1="${lx2}" y1="${sy + 30 + SH + 50}" x2="${lx2 + 22}" y2="${sy + 30 + SH + 50}" stroke="${s.color}" stroke-width="2.5" ${s.dash ? `stroke-dasharray="${s.dash}"` : ""}/>`);
  s2.push(`<text x="${lx2 + 28}" y="${sy + 30 + SH + 54}" font-size="11" fill="#333">${esc(s.label)}</text>`);
  lx2 += 36 + s.label.length * 6.2;
}
s2.push("</svg>");
writeFileSync("results/summary.svg", s2.join("\n"));
console.log(`control drift: ${raw.control.drift_pct.toFixed(2)}%`);
console.log("\ncrossover (lowest % inline where the inline layout wins vs p_hand)");
console.log(
  `${"N".padEnd(14)}${ovfs.map((o) => `${o.label} i4/i1`.padStart(22)).join("")}`,
);
for (const c of caps) {
  const cells = ovfs.map((o) => {
    const a = crossover(c.label, o.label, "i4");
    const b = crossover(c.label, o.label, "i1");
    return `${a === null ? "never" : a === "tie" ? "tie" : `${a}%`} / ${b === null ? "never" : b === "tie" ? "tie" : `${b}%`}`.padStart(22);
  });
  console.log(`${`N=${c.n} ${c.label}`.padEnd(14)}${cells.join("")}`);
}

console.log("\nbranch cost at 100% inline (i minus h, ns/row)");
for (const c of caps) {
  const o = ovfs[0].label;
  const i4 = pick(c.label, o, "i4").get(100);
  const h4 = pick(c.label, o, "h4").get(100);
  const i1 = pick(c.label, o, "i1").get(100);
  const h1 = pick(c.label, o, "h1").get(100);
  if (i4 && h4 && i1 && h1) {
    console.log(
      `  N=${String(c.n).padEnd(5)} 4N+4: ${(i4.ns_per_row - h4.ns_per_row).toFixed(2).padStart(7)}  (${((i4.ns_per_row / h4.ns_per_row - 1) * 100).toFixed(1)}%)   N+4: ${(i1.ns_per_row - h1.ns_per_row).toFixed(2).padStart(7)}  (${((i1.ns_per_row / h1.ns_per_row - 1) * 100).toFixed(1)}%)`,
    );
  }
}
console.log("\nstorage amplification at 50% inline (bytes on disk vs pointer)");
for (const c of caps) {
  const cells = ovfs.map((o) => {
    const a = amp(c.label, o.label, "i4", 50);
    const b = amp(c.label, o.label, "i1", 50);
    return `${a ? `${a.toFixed(2)}x` : "—"} / ${b ? `${b.toFixed(2)}x` : "—"}`.padStart(20);
  });
  console.log(`${`N=${c.n} ${c.label}`.padEnd(14)}${cells.join("")}`);
}
console.log("\nwrote results/grid.svg and results/summary.svg");
