#!/usr/bin/env bun
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
const [src, dst, scaleArg] = process.argv.slice(2);
if (!src || !dst) {
  console.error("usage: bun svg2png.ts <in.svg> <out.png> [scale]");
  process.exit(1);
}
const scale = Number(scaleArg ?? 2);
const svg = readFileSync(src, "utf8");
const w = Number(/width="(\d+(?:\.\d+)?)"/.exec(svg)?.[1] ?? 1200);
const h = Number(/height="(\d+(?:\.\d+)?)"/.exec(svg)?.[1] ?? 800);
const work = mkdtempSync(join(tmpdir(), "svg2png-"));
const page = join(work, "page.html");
writeFileSync(
  page,
  `<!doctype html><meta charset="utf-8">
<style>html,body{margin:0;padding:0;background:#fff}svg{display:block}</style>
${svg}`,
);
const chrome =
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const r = spawnSync(
  chrome,
  [
    "--headless",
    "--disable-gpu",
    "--hide-scrollbars",
    "--force-device-scale-factor=" + scale,
    `--window-size=${Math.ceil(w)},${Math.ceil(h)}`,
    `--screenshot=${join(work, "shot.png")}`,
    `file://${page}`,
  ],
  { stdio: "inherit" },
);
if (r.status !== 0) {
  console.error(`chrome exited ${r.status}`);
  process.exit(1);
}
writeFileSync(dst, readFileSync(join(work, "shot.png")));
rmSync(work, { recursive: true, force: true });
console.log(`${dst} — ${Math.ceil(w * scale)}x${Math.ceil(h * scale)}`);
