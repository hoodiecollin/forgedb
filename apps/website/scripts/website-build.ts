import { existsSync, mkdirSync, renameSync, rmSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
const ROUTE_DIR = path.join(process.cwd(), "app", "api", "dev-rewrite");
const STASH_DIR = path.join(process.cwd(), ".rewrite-queue", "_route-stash");
function stash(): void {
  mkdirSync(path.dirname(STASH_DIR), { recursive: true });
  rmSync(STASH_DIR, { recursive: true, force: true });
  renameSync(ROUTE_DIR, STASH_DIR);
}
function restore(): void {
  if (!existsSync(STASH_DIR)) return;
  mkdirSync(path.dirname(ROUTE_DIR), { recursive: true });
  rmSync(ROUTE_DIR, { recursive: true, force: true });
  renameSync(STASH_DIR, ROUTE_DIR);
}
if (!existsSync(ROUTE_DIR) && existsSync(STASH_DIR)) {
  restore();
  console.log("• recovered dev rewrite route from a prior interrupted build");
}
const stashed = existsSync(ROUTE_DIR);
if (stashed) {
  stash();
  console.log("• stashed dev rewrite route (incompatible with output: export)");
}
for (const sig of ["SIGINT", "SIGTERM"] as const) {
  process.on(sig, () => {
    if (stashed) restore();
    process.exit(sig === "SIGINT" ? 130 : 143);
  });
}
try {
  const res = spawnSync("bun", ["run", "build"], { stdio: "inherit" });
  process.exitCode = res.status ?? 1;
} finally {
  if (stashed) {
    restore();
    console.log("• restored dev rewrite route");
  }
}
