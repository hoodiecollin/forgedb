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
 *
 * Two exit paths, deliberately distinct:
 *   • watcher exits on its own (a rewrite was queued) → the dev server is LEFT UP
 *     for the next wake cycle.
 *   • Ctrl-C / SIGTERM (you're done) → the whole tree is TORN DOWN, including the
 *     detached dev server. Its pid is recorded in `.next/dev-server.pid` so even a
 *     later run that only reused the server can still stop it on Ctrl-C.
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";

const PORT = 3100;
const LOG = path.join(process.cwd(), ".next", "dev-server.log");
const PIDFILE = path.join(process.cwd(), ".next", "dev-server.pid");

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

// The detached dev server's pid — either the one we start below, or (on reuse) the
// one recorded by an earlier run — so a Ctrl-C in this run can still tear it down.
let devPid: number | undefined;

if (await portOpen(PORT)) {
  console.log(`✓ dev server already running on http://localhost:${PORT} — reusing it`);
  try {
    const recorded = Number.parseInt(fs.readFileSync(PIDFILE, "utf8").trim(), 10);
    if (Number.isInteger(recorded) && recorded > 0) devPid = recorded;
  } catch {
    // no pidfile (server started by some other means) — Ctrl-C will still stop the
    // watcher; the foreign server is left alone.
  }
} else {
  console.log(`▶ starting Next dev server on http://localhost:${PORT} (background)…`);
  fs.mkdirSync(path.dirname(LOG), { recursive: true });
  // Detached + own log file so it survives this command's exit (each wake cycle)
  // and its output never pollutes the watcher's foreground (which Claude parses).
  // `detached: true` also makes it a process-group leader, so on Ctrl-C we can kill
  // the whole tree (next dev → next-server → build worker) with one group signal.
  const logFd = fs.openSync(LOG, "a");
  const dev = spawn("bun", ["run", "dev"], { stdio: ["ignore", logFd, logFd], detached: true });
  dev.unref();
  devPid = dev.pid;
  if (devPid) fs.writeFileSync(PIDFILE, String(devPid));
  await waitForPort(PORT);
  console.log(
    `✓ dev server ready (pid ${devPid}) — logs: ${path.relative(process.cwd(), LOG)}\n` +
      `  it stays up across wake cycles; Ctrl-C here tears it down (or:  kill ${devPid})`,
  );
}

console.log("▶ arming the rewrite wake watcher (⌥E in the browser to edit)…\n");
const watch = spawn("bun", ["scripts/rewrite-watch.ts"], { stdio: "inherit" });

let shuttingDown = false;

/** Ctrl-C / SIGTERM: tear down the watcher AND the detached dev server tree. */
function shutdown(): void {
  if (shuttingDown) return;
  shuttingDown = true;
  watch.removeAllListeners("exit");
  try {
    watch.kill("SIGTERM");
  } catch {
    // already gone
  }
  if (devPid !== undefined) {
    try {
      // Negative pid → signal the whole process group led by the detached server.
      process.kill(-devPid, "SIGTERM");
    } catch {
      // stale/already-dead group — nothing to reap
    }
    try {
      fs.rmSync(PIDFILE, { force: true });
    } catch {
      // best effort
    }
  }
  process.exit(0);
}

watch.on("exit", (code, signal) => {
  // A signal-killed watcher (Ctrl-C propagated to the foreground group) is a
  // shutdown, not a rewrite-queued exit — tear the server down too. A clean exit
  // (rewrite queued) leaves the dev server up for the next wake cycle.
  if (signal) return shutdown();
  process.exit(code ?? 0);
});

for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"] as const) {
  process.on(sig, shutdown);
}
