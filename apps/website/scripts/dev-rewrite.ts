import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
const PORT = 3100;
const LOG = path.join(process.cwd(), ".next", "dev-server.log");
const PIDFILE = path.join(process.cwd(), ".next", "dev-server.pid");
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
let devPid: number | undefined;
if (await portOpen(PORT)) {
  console.log(`✓ dev server already running on http://localhost:${PORT} — reusing it`);
  try {
    const recorded = Number.parseInt(fs.readFileSync(PIDFILE, "utf8").trim(), 10);
    if (Number.isInteger(recorded) && recorded > 0) devPid = recorded;
  } catch {

  }
} else {
  console.log(`▶ starting Next dev server on http://localhost:${PORT} (background)…`);
  fs.mkdirSync(path.dirname(LOG), { recursive: true });

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
function shutdown(): void {
  if (shuttingDown) return;
  shuttingDown = true;
  watch.removeAllListeners("exit");
  try {
    watch.kill("SIGTERM");
  } catch {
  }
  if (devPid !== undefined) {
    try {
      process.kill(-devPid, "SIGTERM");
    } catch {
    }
    try {
      fs.rmSync(PIDFILE, { force: true });
    } catch {
    }
  }
  process.exit(0);
}
watch.on("exit", (code, signal) => {
  if (signal) return shutdown();
  process.exit(code ?? 0);
});
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"] as const) {
  process.on(sig, shutdown);
}
