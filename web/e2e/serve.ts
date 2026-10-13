// Global setup: a temp SWARM_HOME with synthetic chats, and the dev `swarm serve` on it. One chat
// has a live chair on a stand-in adapter, so chat actions run the real CLI with no provider.
// E2E_SWARM and E2E_TOOL point at other builds of the two binaries, relative to the repo root.

import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { randomUUID } from "node:crypto";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
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
const PANES = ["fixture-chair", ...TAB_PANES, ...PANE_PROJECTS.flatMap((p) => ["chair", "reviewer", "tester"].map((a) => paneOf(p, a)))];

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

/** Issues that the stand-in `gh` lists: one per browser, so each makes its own workspace. */
export const ISSUES = [
  { number: 7, title: "Fix Sidebar", body: "Use less space in the sidebar." },
  { number: 8, title: "Tidy Tabs", body: "Close a tab when its chat is archived." },
];

/** A stand-in `gh` for the lifecycle cases: a pull request view, the issue list and each issue.
 *  The JSON has no single quote, so it sits in shell single quotes. */
const issueJson = (i: (typeof ISSUES)[number]) => ({ ...i, url: `https://github.com/acme/demo/issues/${i.number}`, state: "OPEN" });
const GH = [
  "#!/bin/sh",
  `if [ "$1 $2" = "pr view" ]; then printf '{"number":%s}\\n' "$3"; exit 0; fi`,
  `if [ "$1 $2" = "issue list" ]; then printf '%s\\n' '${JSON.stringify(ISSUES.map((i) => ({ ...issueJson(i), labels: [] })))}'; exit 0; fi`,
  ...ISSUES.map((i) => `if [ "$1 $2 $3" = "issue view ${i.number}" ]; then printf '%s\\n' '${JSON.stringify(issueJson(i))}'; exit 0; fi`),
  'echo "unexpected gh $*" >&2; exit 1',
  "",
].join("\n");

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

  // Two Git projects for the lifecycle cases. proj-gh has a GitHub origin, for issues, and a free
  // branch per browser. proj-pr has a local origin that holds pull request 7, since `git fetch`
  // cannot reach github.com here. The server finds `gh` first on its PATH.
  const gitEnv = { ...env, GIT_CEILING_DIRECTORIES: dir };
  const git = (cwd: string, ...args: string[]) => execFileSync("git", ["-c", "user.name=E2E", "-c", "user.email=e2e@example.invalid", ...args], { cwd, env: gitEnv, stdio: "pipe" });
  const ghProject = join(dir, "proj-gh");
  const prProject = join(dir, "proj-pr");
  const origin = join(dir, "pr-origin.git");
  mkdirSync(ghProject);
  git(ghProject, "init", "-q", "-b", "main");
  writeFileSync(join(ghProject, "README.md"), "demo\n");
  git(ghProject, "add", "README.md");
  git(ghProject, "commit", "-q", "-m", "first");
  for (const project of PANE_PROJECTS) git(ghProject, "branch", `topic-${project}`);
  git(ghProject, "checkout", "-q", "-b", "pr-7");
  writeFileSync(join(ghProject, "review.md"), "the change under review\n");
  git(ghProject, "add", "review.md");
  git(ghProject, "commit", "-q", "-m", "change under review");
  git(dir, "init", "-q", "--bare", "-b", "main", origin);
  git(ghProject, "push", "-q", origin, "main", "pr-7:refs/pull/7/head");
  git(ghProject, "checkout", "-q", "main");
  git(ghProject, "branch", "-q", "-D", "pr-7");
  git(ghProject, "remote", "add", "origin", "https://github.com/acme/demo.git");
  git(dir, "clone", "-q", origin, prProject);
  const bin = join(dir, "bin");
  mkdirSync(bin);
  writeFileSync(join(bin, "gh"), GH);
  chmodSync(join(bin, "gh"), 0o755);
  env.PATH = `${bin}:${process.env.PATH}`;

  server = spawn(swarm, ["serve", "--port", "0", "--assets", resolve(import.meta.dirname, "../dist")], {
    env,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const first = await new Promise<string>((ok, fail) => {
    createInterface({ input: server!.stdout! }).once("line", ok);
    server!.once("exit", (code) => fail(new Error(`swarm serve exited with ${code}`)));
  });
  process.env.SWARM_SERVE_URL = first.trim();
  const url = new URL(first.trim());
  const token = new URLSearchParams(url.hash.slice(1)).get("token");
  for (const path of [ghProject, prProject]) {
    const res = await fetch(`${url.origin}/api/project/import`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({ path }),
    });
    if (!res.ok) throw new Error(`import ${path}: ${res.status} ${await res.text()}`);
  }
  process.env.SWARM_E2E_HOME = home;
  process.env.SWARM_E2E_LIVE = liveSession;
  process.env.SWARM_E2E_DIR = dir;
  return async () => {
    server?.kill();
    rmSync(dir, { recursive: true, force: true });
  };
}
