#!/usr/bin/env bun
export const PERMANENT_BRANCHES = ["main", "develop"] as const;

export function isPermanentBranch(name: string): boolean {
  return (PERMANENT_BRANCHES as readonly string[]).includes(name.trim());
}

export function refExists(lsRemoteOutput: string): boolean {
  return lsRemoteOutput.trim().length > 0;
}

export function prHeadFailure(base: string, head: string): string {
  const suggestion = base.trim() === "main" ? "release/vX.Y.Z" : "sync/<what-it-carries>";
  return [
    `✗ This pull request's head branch is \`${head}\`, which is permanent.`,
    ``,
    `    Merging it DELETES \`${head}\`: \`delete_branch_on_merge\` is true, and the \`deletion\``,
    `    rule protecting it is bypassed by the admin role that performs every merge (#487).`,
    `    It has fired on 2 of 2 release merges; the second went unnoticed for five days.`,
    ``,
    `    Cut a disposable head instead, then open the PR from that:`,
    ``,
    `        git push origin origin/${head}:refs/heads/${suggestion}`,
    `        gh pr create --base ${base} --head ${suggestion}`,
  ].join("\n");
}

export function missingBranchFailure(name: string): string {
  return [
    `✗ \`${name}\` does not exist on origin.`,
    ``,
    `    A release merge headed by a permanent branch deletes it (#487), and a \`git fetch\``,
    `    without \`--prune\` keeps serving a stale \`origin/${name}\` that logs and diffs`,
    `    normally — so this is invisible to every read you would naturally run.`,
    ``,
    `    Recreate it at the commit it should carry:`,
    ``,
    `        git push origin <sha>:refs/heads/${name}`,
  ].join("\n");
}

async function lsRemote(name: string): Promise<string> {
  const proc = Bun.spawn(["git", "ls-remote", "--heads", "origin", name], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
  if (code !== 0) throw new Error(`git ls-remote exited ${code}`);
  return out;
}

async function main(argv: string[]): Promise<number> {
  const [cmd, ...rest] = argv;
  if (cmd === "pr-head") {
    const [base, head] = rest;
    if (!base || !head) {
      console.error("usage: check-branch-hygiene.ts pr-head <base> <head>");
      return 2;
    }
    if (isPermanentBranch(head)) {
      console.error(prHeadFailure(base, head));
      return 1;
    }
    console.log(`✓ head \`${head}\` is disposable; merging it cannot delete a permanent branch.`);
    return 0;
  }
  if (cmd === "branch-exists") {
    const [name] = rest;
    if (!name) {
      console.error("usage: check-branch-hygiene.ts branch-exists <name>");
      return 2;
    }
    if (!refExists(await lsRemote(name))) {
      console.error(missingBranchFailure(name));
      return 1;
    }
    console.log(`✓ \`${name}\` exists on origin.`);
    return 0;
  }
  console.error("usage: check-branch-hygiene.ts <pr-head|branch-exists> ...");
  return 2;
}

if (import.meta.main) process.exit(await main(Bun.argv.slice(2)));
