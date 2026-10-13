// Global setup: a temp SWARM_HOME with synthetic chats, and the dev `swarm serve` on it. One chat
// has a live chair on a stand-in adapter, so chat actions run the real CLI with no provider.
// E2E_SWARM and E2E_TOOL point at other builds of the two binaries, relative to the repo root.

import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { randomUUID } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

const repo = resolve(import.meta.dirname, "../..");
const swarm = resolve(repo, process.env.E2E_SWARM ?? "target/debug/swarm");
const tool = resolve(repo, process.env.E2E_TOOL ?? "packages/transcript/zig-out/bin/transcript");
const fixtures = join(repo, "packages/transcript/src/fixtures");

/** A synthetic Claude log: one prompt, then one Bash call whose output has `lines` lines. */
function giantLog(session: string, lines: number): string {
  const at = new Date().toISOString();
  const output = Array.from({ length: lines }, (_, i) => `line ${i} of the build log`).join("\n");
  return [
    { type: "user", uuid: "u-1", parentUuid: null, sessionId: session, timestamp: at, message: { role: "user", content: "Show the build log" } },
    { type: "assistant", uuid: "a-1", parentUuid: "u-1", sessionId: session, timestamp: at, message: { role: "assistant", model: "claude-opus-4-6", content: [{ type: "tool_use", id: "toolu-big", name: "Bash", input: { command: "cat big.log" } }] } },
    { type: "user", uuid: "u-2", parentUuid: "a-1", sessionId: session, timestamp: at, message: { role: "user", content: [{ type: "tool_result", tool_use_id: "toolu-big", content: output }] } },
  ]
    .map((line) => JSON.stringify(line))
    .join("\n") + "\n";
}
export const GIANT_LINES = 20_000;

/** The browsers that run the pane case. Each gets its own chat, because the case closes an agent. */
export const PANE_PROJECTS = ["chromium", "webkit"];
const paneOf = (project: string, agent: string) => `fixture-${project}-${agent}`;
/** Two live chats in one workspace, for the tab strip cases. */
const TAB_PANES = ["fixture-tabs-1", "fixture-tabs-2"];
/** The live chat in the git workspace that the Changes cases send diff comments to. */
const GIT_PANE = "fixture-git";
const PANES = ["fixture-chair", ...TAB_PANES, GIT_PANE, ...PANE_PROJECTS.flatMap((p) => ["chair", "reviewer", "tester"].map((a) => paneOf(p, a)))];

/** A pane adapter that is only shell lines: it logs each verb under $HOME and reads a pane's screen
 *  from $HOME/screen-<pane>, else from $HOME/screen. While $HOME/fail exists, typing fails with its
 *  text. An agent registers on the pane that FIXTURE_PANE names, and a closed pane leaves the list. */
const FIXTURE_ADAPTER = `self = printf '%s' "\${FIXTURE_PANE:-fixture-chair}"
spawn = printf fixture-new
list = for p in ${PANES.join(" ")}; do [ -e "$HOME/closed-$p" ] || printf '%s sh\\n' "$p"; done
ring = if [ -e "$HOME/fail" ]; then cat "$HOME/fail" >&2; exit 1; fi; printf '%s\\n' "$SWARM_TEXT" >> "$HOME/typed"
interrupt = printf 'interrupt %s\\n' "$SWARM_PANE" >> "$HOME/actions"
key = printf 'key %s %s\\n' "$SWARM_PANE" "$SWARM_KEY" >> "$HOME/actions"
close = printf 'close %s\\n' "$SWARM_PANE" >> "$HOME/actions"; touch "$HOME/closed-$SWARM_PANE"
capture = cat "$HOME/screen-$SWARM_PANE" 2>/dev/null || cat "$HOME/screen"
screen = cat "$HOME/screen-$SWARM_PANE" 2>/dev/null || cat "$HOME/screen"
`;

/** A stand-in `gh`: the PR cases write $HOME/pr-list and $HOME/pr-view. */
const GH = `#!/bin/sh
case "$1 $2" in
 'repo view') printf '{"nameWithOwner":"example/project","parent":null}\\n' ;;
 'pr list') cat "$HOME/pr-list" 2>/dev/null || printf '[]\\n' ;;
 'pr view') cat "$HOME/pr-view" ;;
 *) exit 1 ;;
esac
`;
export const GIANT_DIFF_LINES = 20_000;

/** A git workspace on branch `feature`, one commit ahead of origin/main, with a staged, an
 *  unstaged and a 20,000-line untracked file. Its origin is GitHub, for the PR lookup. */
function gitWorkspace(cwd: string, env: NodeJS.ProcessEnv) {
  const gitEnv = { ...env, GIT_AUTHOR_NAME: "E2E", GIT_AUTHOR_EMAIL: "e2e@example.com", GIT_COMMITTER_NAME: "E2E", GIT_COMMITTER_EMAIL: "e2e@example.com" };
  const git = (...args: string[]) => execFileSync("git", ["-C", cwd, ...args], { env: gitEnv, stdio: "pipe" });
  const write = (path: string, text: string) => {
    mkdirSync(join(cwd, path, ".."), { recursive: true });
    writeFileSync(join(cwd, path), text);
  };
  const app = Array.from({ length: 30 }, (_, i) => `export const value${i + 1} = ${i + 1};`);
  git("init", "-q", "-b", "main");
  write("src/app.ts", app.join("\n") + "\n");
  write("README.md", "# Project\n\nA test repository.\n");
  git("add", ".");
  git("commit", "-q", "-m", "base");
  git("remote", "add", "origin", "https://github.com/example/project.git");
  git("update-ref", "refs/remotes/origin/main", "HEAD");
  git("switch", "-q", "-c", "feature");
  app.splice(9, 2, "export const value10 = 100;", "export const value11 = 110;", "export const added = true;");
  write("src/app.ts", app.join("\n") + "\n");
  git("commit", "-q", "-am", "feature");
  write("README.md", "# Project\n\nA test repository, changed.\n");
  write("notes.md", "staged notes\n");
  git("add", "notes.md");
  write("big.txt", Array.from({ length: GIANT_DIFF_LINES }, (_, i) => `generated line ${i + 1}`).join("\n") + "\n");
}

let server: ChildProcess | undefined;

export default async function setup() {
  const dir = mkdtempSync(join(tmpdir(), "swarm-web-e2e-"));
  const home = join(dir, "home");
  // Only what the server needs: no host adapter variables, no owner config, no real logs.
  // The stand-in gh comes first on PATH, so no case reaches GitHub.
  mkdirSync(join(dir, "bin"));
  writeFileSync(join(dir, "bin/gh"), GH, { mode: 0o755 });
  const env: NodeJS.ProcessEnv = {
    PATH: `${join(dir, "bin")}:${process.env.PATH}`,
    HOME: home,
    SWARM_HOME: home,
    CLAUDE_CONFIG_DIR: join(home, ".claude"),
    CODEX_HOME: join(home, ".codex"),
    SWARM_TRANSCRIPT_TOOL: tool,
  };
  const logs = join(home, ".claude/projects/e2e");
  mkdirSync(logs, { recursive: true });
  // Chats with no adapter. The tree lists only chats with agents, so each gets an ended chair below.
  const plain: { session: string; log: string }[] = [];
  const lane = (cwd: string, chair: string) => {
    const session = execFileSync(swarm, ["session", "new", "lane", "--chair", `claude:${chair}`], { cwd, env, stdio: "pipe" }).toString().trim().split("\n").pop()!;
    plain.push({ session, log: join(logs, `${chair}.jsonl`) });
  };
  for (const [ws, fixture] of [
    ["ws-build", "claude-chair-run.jsonl"],
    ["ws-edit", "claude-edit.jsonl"],
    ["ws-giant", null],
  ]) {
    const cwd = join(dir, ws!);
    mkdirSync(cwd);
    const chair = randomUUID();
    if (fixture) copyFileSync(join(fixtures, fixture), join(logs, `${chair}.jsonl`));
    else writeFileSync(join(logs, `${chair}.jsonl`), giantLog(chair, GIANT_LINES));
    lane(cwd, chair);
  }
  // The live chat: its chair registers from the stand-in pane, as a chair CLI does at start.
  mkdirSync(join(home, ".swarm/adapters"), { recursive: true });
  writeFileSync(join(home, ".swarm/adapters/fixture.conf"), FIXTURE_ADAPTER);
  copyFileSync(join(repo, "tests/fixtures/screens/claude-idle.txt"), join(home, "screen"));
  const live = join(dir, "ws-live");
  mkdirSync(live);
  const liveChair = randomUUID();
  copyFileSync(join(fixtures, "claude-edit.jsonl"), join(logs, `${liveChair}.jsonl`));
  const liveEnv = { ...env, SWARM_ADAPTER: "fixture" };
  const liveSession = execFileSync(swarm, ["session", "new", "lane", "--chair", `claude:${liveChair}`], { cwd: live, env: liveEnv, stdio: "pipe" })
    .toString()
    .trim()
    .split("\n")
    .pop()!;
  execFileSync(swarm, ["agent", "add", "orchestrator", "orchestrator"], { env: { ...liveEnv, SWARM_SESSION_ID: liveSession }, stdio: "pipe" });
  // Two live chats in ws-tabs. A tab of an ended chat closes when the owner leaves it (TabStrip),
  // so the tab cases need chats whose chairs are alive.
  const tabsDir = join(dir, "ws-tabs");
  mkdirSync(tabsDir);
  for (const [i, pane] of TAB_PANES.entries()) {
    const chairLog = randomUUID();
    copyFileSync(join(fixtures, i ? "claude-chair-run.jsonl" : "claude-edit.jsonl"), join(logs, `${chairLog}.jsonl`));
    const session = execFileSync(swarm, ["session", "new", "lane", "--chair", `claude:${chairLog}`], { cwd: tabsDir, env: liveEnv, stdio: "pipe" }).toString().trim().split("\n").pop()!;
    execFileSync(swarm, ["agent", "add", "orchestrator", "orchestrator"], { env: { ...liveEnv, SWARM_SESSION_ID: session, FIXTURE_PANE: pane }, stdio: "pipe" });
  }
  const gitDir = join(dir, "ws-git");
  mkdirSync(gitDir);
  gitWorkspace(gitDir, env);
  const gitLog = randomUUID();
  copyFileSync(join(fixtures, "claude-edit.jsonl"), join(logs, `${gitLog}.jsonl`));
  const gitSession = execFileSync(swarm, ["session", "new", "lane", "--chair", `claude:${gitLog}`], { cwd: gitDir, env: liveEnv, stdio: "pipe" }).toString().trim().split("\n").pop()!;
  execFileSync(swarm, ["agent", "add", "orchestrator", "orchestrator"], { env: { ...liveEnv, SWARM_SESSION_ID: gitSession, FIXTURE_PANE: GIT_PANE }, stdio: "pipe" });
  process.env.SWARM_E2E_GIT = gitSession;
  process.env.SWARM_E2E_GIT_DIR = gitDir;
  // Each pane of the pane chats has its own screen, so the chair case's question does not reach them.
  for (const pane of PANES.slice(1)) copyFileSync(join(repo, "tests/fixtures/screens/claude-idle.txt"), join(home, `screen-${pane}`));
  // One chat per browser with a chair and two children, each on its own stand-in pane. `swarm
  // spawn` gives a child its pane and its hooks report its log; here the store gets both directly.
  const db = execFileSync("fd", ["-HI", "-t", "f", "^swarm.db$", home], { encoding: "utf8" }).trim().split("\n")[0];
  const sql = (text: string) => `'${text.replaceAll("'", "''")}'`;
  for (const { session, log } of plain)
    execFileSync("sqlite3", [db, `INSERT INTO agent (id, session_id, role, provider, created_at, log) VALUES ('orchestrator', ${sql(session)}, 'orchestrator', 'claude', ${Math.floor(Date.now() / 1000)}, ${sql(log)});`]);
  for (const project of PANE_PROJECTS) {
    const cwd = join(dir, `ws-panes-${project}`);
    mkdirSync(cwd);
    const chairLog = randomUUID();
    copyFileSync(join(fixtures, "claude-edit.jsonl"), join(logs, `${chairLog}.jsonl`));
    const session = execFileSync(swarm, ["session", "new", "lane", "--chair", `claude:${chairLog}`], { cwd, env: liveEnv, stdio: "pipe" })
      .toString()
      .trim()
      .split("\n")
      .pop()!;
    const add = (agent: string, role: string) =>
      execFileSync(swarm, ["agent", "add", agent, role], { env: { ...liveEnv, SWARM_SESSION_ID: session, FIXTURE_PANE: paneOf(project, "chair") }, stdio: "pipe" });
    add("orchestrator", "orchestrator");
    for (const [agent, fixture] of [
      ["reviewer", "claude-chair-run.jsonl"],
      ["tester", "claude-edit.jsonl"],
    ]) {
      add(agent, "code.small");
      const log = join(logs, `${randomUUID()}.jsonl`);
      copyFileSync(join(fixtures, fixture), log);
      execFileSync("sqlite3", [db, `UPDATE agent SET log = ${sql(log)}, provider = 'claude', pane_id = ${sql(paneOf(project, agent))} WHERE session_id = ${sql(session)} AND id = ${sql(agent)};`]);
    }
    process.env[`SWARM_E2E_PANES_${project}`] = session;
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
  process.env.SWARM_E2E_LIVE = liveSession;
  process.env.SWARM_E2E_DIR = dir;
  return async () => {
    server?.kill();
    rmSync(dir, { recursive: true, force: true });
  };
}
