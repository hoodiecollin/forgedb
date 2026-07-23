/**
 * Wraps `next build` so a LOCAL static-export build tolerates the gitignored
 * dev-only prose-rewrite route. The POST handler at `app/api/dev-rewrite/` is
 * incompatible with `output: "export"`, so its mere presence fails the build.
 * Here we stash it *out of the app tree* for the duration of the build and
 * restore it afterward — including on build failure, ctrl-C, or a crashed prior
 * run (recovered on next invocation).
 *
 * On CI / a fresh checkout the route never exists (it is gitignored), so this is
 * a plain `next build` no-op. Invoked by `make website-build`.
 */
import { existsSync, mkdirSync, renameSync, rmSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const ROUTE_DIR = path.join(process.cwd(), "app", "api", "dev-rewrite");
// Stash outside app/ (a name inside app/ would still be scanned as a route) and
// inside the already-gitignored queue dir.
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

// Recover from a prior interrupted build that left the route stashed.
if (!existsSync(ROUTE_DIR) && existsSync(STASH_DIR)) {
  restore();
  console.log("• recovered dev rewrite route from a prior interrupted build");
}

const stashed = existsSync(ROUTE_DIR);
if (stashed) {
  stash();
  console.log("• stashed dev rewrite route (incompatible with output: export)");
}

// Restore on signals too — a `finally` doesn't run on SIGINT/SIGTERM.
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
