import { describe, expect, test } from "bun:test";
import {
  PERMANENT_BRANCHES,
  isPermanentBranch,
  missingBranchFailure,
  prHeadFailure,
  refExists,
} from "./check-branch-hygiene";

const SCRIPT = new URL("./check-branch-hygiene.ts", import.meta.url).pathname;

async function run(args: string[]): Promise<{ code: number; out: string; err: string }> {
  const proc = Bun.spawn(["bun", SCRIPT, ...args], { stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { code, out, err };
}

describe("pr-head", () => {
  test("scenario 1: the release merge, head develop into main, is refused", async () => {
    const r = await run(["pr-head", "main", "develop"]);
    expect(r.code).toBe(1);
    expect(r.err).toContain("head branch is `develop`");
    expect(r.err).toContain("release/vX.Y.Z");
  });

  test("scenario 2: the forward merge, head main into develop, is refused", async () => {
    const r = await run(["pr-head", "develop", "main"]);
    expect(r.code).toBe(1);
    expect(r.err).toContain("head branch is `main`");
  });

  test("scenario 3: a sync/* head is allowed", async () => {
    const r = await run(["pr-head", "develop", "sync/main-to-develop-542"]);
    expect(r.code).toBe(0);
    expect(r.out).toContain("disposable");
  });

  test("scenario 4: an ordinary feature head is allowed", async () => {
    const r = await run(["pr-head", "develop", "fix/487-branch-hygiene"]);
    expect(r.code).toBe(0);
  });

  test("both permanent branches are refused, so neither direction is missed", () => {
    for (const b of PERMANENT_BRANCHES) expect(isPermanentBranch(b)).toBe(true);
    expect(PERMANENT_BRANCHES).toContain("main");
    expect(PERMANENT_BRANCHES).toContain("develop");
  });

  test("a branch merely named like a permanent one is not refused", () => {
    expect(isPermanentBranch("develop-2")).toBe(false);
    expect(isPermanentBranch("feat/main")).toBe(false);
    expect(isPermanentBranch("mainline")).toBe(false);
  });

  test("the refusal names the remedy for the base it was given", () => {
    expect(prHeadFailure("main", "develop")).toContain("release/vX.Y.Z");
    expect(prHeadFailure("develop", "main")).toContain("sync/");
  });

  test("a missing argument is a usage error, not a pass", async () => {
    expect((await run(["pr-head", "main"])).code).toBe(2);
    expect((await run([])).code).toBe(2);
  });
});

describe("branch-exists", () => {
  test("scenario 5: a present ref reads as present", () => {
    expect(refExists("4e36629755a03a44430504015b24be789a252e0d\trefs/heads/develop\n")).toBe(true);
  });

  test("scenario 6: an absent ref reads as absent and the message names the recreate", () => {
    expect(refExists("")).toBe(false);
    expect(refExists("   \n  ")).toBe(false);
    const msg = missingBranchFailure("develop");
    expect(msg).toContain("does not exist on origin");
    expect(msg).toContain("git push origin <sha>:refs/heads/develop");
  });

  test("scenario 6 induced end to end: a name that cannot exist exits 1", async () => {
    const r = await run(["branch-exists", "zzz-absent-by-construction-487"]);
    expect(r.code).toBe(1);
    expect(r.err).toContain("does not exist on origin");
  });
});
