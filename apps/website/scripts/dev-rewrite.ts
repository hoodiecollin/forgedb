/**
 * Bundles the prose-rewrite dev loop into one command: ensures the Next dev
 * server is up (starting it in the background if the port is free), then runs the
 * wake watcher in the foreground. LOCAL DEV ONLY.
 *
 *   bun scripts/dev-rewrite.ts        # or, from the repo root: make website-rewrite
 *
 * The watcher exits when a rewrite request is queued, so this whole command exits
 * too — that is the signal for Claude Code to draft the proposal. Re-running is
 * idempotent: the dev server started on the first run stays up across wake cycles
 * (its logs go to `.next/dev-server.log`, not the foreground), and later runs
 * detect the open port and reuse it instead of starting a second server.
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";

const PORT = 3100;
const LOG = path.join(process.cwd(), ".next", "dev-server.log");

/** True if something is already listening on `port` (i.e. the dev server is up). */
function portOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = net.connect({ port, host: "127.0.0.1" });
    const done = (v: boolean) => {
      sock.destroy();
      resolve(v);
    };
    sock.setTimeout(500);
    sock.once("connect", () => done(true));
    sock.once("timeout", () => done(false));
    sock.once("error", () => resolve(false));
  });
}

async function waitForPort(port: number, tries = 120): Promise<void> {
  for (let i = 0; i < tries; i++) {
    if (await portOpen(port)) return;
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`dev server never came up on :${port} — see ${LOG}`);
}

if (await portOpen(PORT)) {
  console.log(`✓ dev server already running on http://localhost:${PORT} — reusing it`);
} else {
  console.log(`▶ starting Next dev server on http://localhost:${PORT} (background)…`);
  fs.mkdirSync(path.dirname(LOG), { recursive: true });
  // Detached + own log file so it survives this command's exit (each wake cycle)
  // and its output never pollutes the watcher's foreground (which Claude parses).
  const logFd = fs.openSync(LOG, "a");
  const dev = spawn("bun", ["run", "dev"], { stdio: ["ignore", logFd, logFd], detached: true });
  dev.unref();
  await waitForPort(PORT);
  console.log(
    `✓ dev server ready (pid ${dev.pid}) — logs: ${path.relative(process.cwd(), LOG)}\n` +
      `  it stays up across wake cycles; stop it when done with:  kill ${dev.pid}`,
  );
}

console.log("▶ arming the rewrite wake watcher (⌥E in the browser to edit)…\n");
const watch = spawn("bun", ["scripts/rewrite-watch.ts"], { stdio: "inherit" });
watch.on("exit", (code) => process.exit(code ?? 0));
