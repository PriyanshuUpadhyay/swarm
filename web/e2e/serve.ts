// Global setup: a temp SWARM_HOME with two synthetic chats, and the dev `swarm serve` on it.
// E2E_SWARM and E2E_TOOL point at other builds of the two binaries, relative to the repo root.

import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { randomUUID } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

const repo = resolve(import.meta.dirname, "../..");
const swarm = resolve(repo, process.env.E2E_SWARM ?? "target/debug/swarm");
const tool = resolve(repo, process.env.E2E_TOOL ?? "packages/transcript/zig-out/bin/transcript");
const fixtures = join(repo, "packages/transcript/src/fixtures");

let server: ChildProcess | undefined;

export default async function setup() {
  const dir = mkdtempSync(join(tmpdir(), "swarm-web-e2e-"));
  const home = join(dir, "home");
  // Only what the server needs: no host adapter variables, no owner config, no real logs.
  const env: NodeJS.ProcessEnv = {
    PATH: process.env.PATH,
    HOME: home,
    SWARM_HOME: home,
    CLAUDE_CONFIG_DIR: join(home, ".claude"),
    CODEX_HOME: join(home, ".codex"),
    SWARM_TRANSCRIPT_TOOL: tool,
  };
  const logs = join(home, ".claude/projects/e2e");
  mkdirSync(logs, { recursive: true });
  for (const [ws, fixture] of [
    ["ws-build", "claude-chair-run.jsonl"],
    ["ws-edit", "claude-edit.jsonl"],
  ]) {
    const cwd = join(dir, ws);
    mkdirSync(cwd);
    const chair = randomUUID();
    copyFileSync(join(fixtures, fixture), join(logs, `${chair}.jsonl`));
    execFileSync(swarm, ["session", "new", "lane", "--chair", `claude:${chair}`], { cwd, env, stdio: "pipe" });
  }
  // `sessions --json` resolves each chair log and stores its path, which serve reads.
  execFileSync(swarm, ["sessions", "--json"], { env, stdio: "pipe" });

  server = spawn(swarm, ["serve", "--port", "0", "--assets", resolve(import.meta.dirname, "../dist")], {
    env,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const first = await new Promise<string>((ok, fail) => {
    createInterface({ input: server!.stdout! }).once("line", ok);
    server!.once("exit", (code) => fail(new Error(`swarm serve exited with ${code}`)));
  });
  process.env.SWARM_SERVE_URL = first.trim();
  process.env.SWARM_E2E_HOME = home;
  return async () => {
    server?.kill();
    rmSync(dir, { recursive: true, force: true });
  };
}
