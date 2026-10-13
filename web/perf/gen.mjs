// Builds a synthetic swarm home for the web pilot budget suite, and can feed live load.
// usage: node gen.mjs setup --home DIR --swarm BIN [--sessions 12] [--history 2000] [--big 100000] [--giant 200000]
//        node gen.mjs live --home DIR [--rate 50] [--seconds 600] [--target panes]
// Only synthetic data: it never reads real agent logs.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const [mode, ...rest] = process.argv.slice(2);
const opt = {};
for (let i = 0; i < rest.length; i += 2) opt[rest[i].replace(/^--/, "")] = rest[i + 1];
const home = path.resolve(opt.home ?? fail("--home is required"));
const manifestPath = path.join(home, "manifest.json");

function fail(message) {
  console.error(message);
  process.exit(2);
}

let uuidCounter = 0;
const ts = () => new Date().toISOString();
const id = (prefix) => `${prefix}-${++uuidCounter}`;

const WORDS = "the agent read the file and ran the tests then fixed a small bug in the parser so the build passes again with one change".split(" ");
function sentence(n) {
  let out = [];
  for (let i = 0; i < n; i++) out.push(WORDS[(i * 7 + uuidCounter) % WORDS.length]);
  return out.join(" ") + ".";
}

// One "event" is one JSONL line in Claude's log format, as in packages/transcript/src/fixtures.
function* turnLines(session, parent) {
  const user = id("u");
  yield { parentUuid: parent, type: "user", message: { role: "user", content: sentence(12) }, uuid: user, timestamp: ts(), sessionId: session };
  const text = id("a");
  const body = `${sentence(20)}\n\n- ${sentence(6)}\n- ${sentence(8)}\n\n\`\`\`js\nconst x = ${uuidCounter};\n\`\`\`\n\n${sentence(30)}`;
  yield { parentUuid: user, type: "assistant", message: { role: "assistant", model: "claude-opus-4-6", content: [{ type: "text", text: body }] }, uuid: text, timestamp: ts(), sessionId: session };
  const call = id("a");
  const toolId = id("toolu");
  yield { parentUuid: text, type: "assistant", message: { role: "assistant", model: "claude-opus-4-6", content: [{ type: "tool_use", id: toolId, name: "Bash", input: { command: "cargo test", description: "Run tests" } }] }, uuid: call, timestamp: ts(), sessionId: session };
  const result = id("u");
  yield { parentUuid: call, type: "user", message: { role: "user", content: [{ type: "tool_result", tool_use_id: toolId, content: Array.from({ length: 8 }, (_, i) => `test case_${i} ... ok`).join("\n") }] }, uuid: result, timestamp: ts(), sessionId: session };
  return result;
}

function writeLines(file, count, session) {
  const fd = fs.openSync(file, "a");
  let parent = null;
  let written = 0;
  let chunk = [];
  while (written < count) {
    const gen = turnLines(session, parent);
    let step;
    while (!(step = gen.next()).done && written < count) {
      chunk.push(JSON.stringify(step.value));
      written++;
    }
    if (step.done) parent = step.value;
    if (chunk.length > 5000) {
      fs.writeSync(fd, chunk.join("\n") + "\n");
      chunk = [];
    }
  }
  if (chunk.length) fs.writeSync(fd, chunk.join("\n") + "\n");
  fs.closeSync(fd);
}

function giantToolLine(file, session, lines) {
  const toolId = id("toolu");
  const call = { parentUuid: null, type: "assistant", message: { role: "assistant", model: "claude-opus-4-6", content: [{ type: "tool_use", id: toolId, name: "Bash", input: { command: "cat big.log" } }] }, uuid: id("a"), timestamp: ts(), sessionId: session };
  const output = Array.from({ length: lines }, (_, i) => `line ${i} ${sentence(6)}`).join("\n");
  const result = { parentUuid: call.uuid, type: "user", message: { role: "user", content: [{ type: "tool_result", tool_use_id: toolId, content: output }] }, uuid: id("u"), timestamp: ts(), sessionId: session };
  fs.appendFileSync(file, JSON.stringify(call) + "\n" + JSON.stringify(result) + "\n");
}

const sql = (s) => `'${String(s).replaceAll("'", "''")}'`;
const git = (cwd, ...args) => execFileSync("git", ["-C", cwd, ...args], { stdio: "ignore" });

function setup() {
  const swarm = opt.swarm ?? fail("--swarm is required");
  const sessions = Number(opt.sessions ?? 12);
  const history = Number(opt.history ?? 2000);
  const big = Number(opt.big ?? 100000);
  const giant = Number(opt.giant ?? 200000);
  fs.rmSync(home, { recursive: true, force: true });
  fs.mkdirSync(path.join(home, "logs"), { recursive: true });
  const env = { ...process.env, SWARM_HOME: home };
  execFileSync(swarm, ["init"], { env, stdio: "ignore" });
  const db = execFileSync("fd", ["-HI", "-t", "f", "^swarm.db$", home], { encoding: "utf8" }).trim().split("\n")[0] || fail("no swarm.db after init");

  // Three projects; the first has a second worktree, so the sidebar has project -> workspace -> chat.
  const workspaces = [];
  // "panes" sorts last, so the rows of the other chats keep their places.
  for (const name of ["alpha", "beta", "gamma", "panes"]) {
    const repo = path.join(home, "repos", name);
    fs.mkdirSync(repo, { recursive: true });
    git(repo, "init", "-q");
    git(repo, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init");
    if (name !== "panes") workspaces.push(repo);
    if (name === "alpha") {
      const wt = path.join(home, "repos", "alpha-feature");
      git(repo, "worktree", "add", "-q", "-b", "feature", wt);
      workspaces.push(wt);
    }
  }

  const rows = [];
  const now = Math.floor(Date.now() / 1000);
  for (let i = 0; i < sessions; i++) {
    const sid = `00000000-0000-7000-8000-${String(i).padStart(12, "0")}`;
    const log = path.join(home, "logs", `chair-${i}.jsonl`);
    const count = i === 0 ? big : history;
    writeLines(log, count, `s${i}`);
    if (i === 1) giantToolLine(log, `s${i}`, giant);
    const cwd = workspaces[i % workspaces.length];
    // adapter stays NULL, so the coordinator lists no panes for synthetic sessions.
    rows.push(`INSERT INTO session (id, talk_mode, cwd, created_at, chair_log, chair_provider, chair_id) VALUES (${sql(sid)}, 'lane', ${sql(cwd)}, ${now - i}, ${sql(log)}, 'claude', ${sql("chair-" + i)});`);
    rows.push(`INSERT INTO agent (id, session_id, role, provider, created_at, log) VALUES (${sql("worker-" + i)}, ${sql(sid)}, 'code.small', 'claude', ${now - i}, ${sql(log)});`);
  }
  // One more chat with four children that have their own logs, for the pane strip's live check.
  const panes = { id: "00000000-0000-7000-8000-00000000aaaa", logs: [path.join(home, "logs", "panes-chair.jsonl")] };
  writeLines(panes.logs[0], history, "panes");
  rows.push(`INSERT INTO session (id, talk_mode, cwd, created_at, chair_log, chair_provider, chair_id) VALUES (${sql(panes.id)}, 'lane', ${sql(path.join(home, "repos", "panes"))}, ${now}, ${sql(panes.logs[0])}, 'claude', 'chair-panes');`);
  for (let k = 0; k < 4; k++) {
    const log = path.join(home, "logs", `pane-${k}.jsonl`);
    writeLines(log, history, `pane${k}`);
    panes.logs.push(log);
    rows.push(`INSERT INTO agent (id, session_id, role, provider, created_at, log, pane_id) VALUES (${sql("pane-" + k)}, ${sql(panes.id)}, 'code.small', 'claude', ${now + k}, ${sql(log)}, ${sql("perf-pane-" + k)});`);
  }
  execFileSync("sqlite3", [db], { input: "BEGIN;\n" + rows.join("\n") + "\nCOMMIT;\n" });
  const manifest = { home, db, panes, sessions: Array.from({ length: sessions }, (_, i) => ({ id: `00000000-0000-7000-8000-${String(i).padStart(12, "0")}`, log: path.join(home, "logs", `chair-${i}.jsonl`) })) };
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));
  console.log(JSON.stringify({ db, sessions, big, giant, history }));
}

// Appends `rate` lines per second to every chair log, in 20 ms ticks, for `seconds`. With
// `--target panes` the logs are those of the pane chat: its chair and its four children.
async function live() {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const rate = Number(opt.rate ?? 50);
  const seconds = Number(opt.seconds ?? 600);
  const logs = opt.target === "panes" ? manifest.panes.logs : manifest.sessions.map((s) => s.log);
  const fds = logs.map((log) => fs.openSync(log, "a"));
  const gens = logs.map((_, i) => ({ gen: turnLines(`s${i}`, null), session: `s${i}` }));
  const tickMs = 20;
  const perTick = rate * tickMs / 1000;
  let carry = 0;
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    carry += perTick;
    const n = Math.floor(carry);
    carry -= n;
    for (let s = 0; s < fds.length; s++) {
      const lines = [];
      for (let k = 0; k < n; k++) {
        let step = gens[s].gen.next();
        if (step.done) {
          gens[s].gen = turnLines(gens[s].session, step.value);
          step = gens[s].gen.next();
        }
        lines.push(JSON.stringify(step.value));
      }
      if (lines.length) fs.writeSync(fds[s], lines.join("\n") + "\n");
    }
    await new Promise((r) => setTimeout(r, tickMs));
  }
  fds.forEach((fd) => fs.closeSync(fd));
}

if (mode === "setup") setup();
else if (mode === "live") await live();
else fail("usage: gen.mjs setup|live --home DIR ...");
