// Run after cargo build --locked --bin swarm. Set PI_SDK_DIR to the installed 1.5.x package.
// Nothing in CI runs it; the Pi SDK is not installed there.
// The `pi` fixture uses Pi's real SDK and RPC mode with a deterministic provider; no paid turns.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, existsSync, watch } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const sdk = process.env.PI_SDK_DIR;
assert(sdk, "Set PI_SDK_DIR to the installed @earendil-works/pi-coding-agent package");
const from = (path) => import(pathToFileURL(path));
const { createAgentSessionRuntime, createAgentSessionFromServices, createAgentSessionServices,
  ModelRuntime, SessionManager, SettingsManager, runRpcMode } = await from(join(sdk, "dist/index.js"));
const { fauxProvider, fauxAssistantMessage, fauxToolCall } = await from(join(sdk, "../pi-ai/dist/index.js"));
const bin = resolve(process.env.SWARM_TEST_BIN ?? "target/debug/swarm");
const models = ["fixture-haiku", "fixture-fable-5", "fixture-astra-2"];
const turn = (value) => fauxAssistantMessage(value);
const call = (name, args, id) => turn(fauxToolCall(name, args, { id }));

async function runtime({ dir, cwd, argv, model, name = "fixture-haiku" }) {
  const faux = fauxProvider({ models: models.map((id) => ({ id })) });
  const modelRuntime = await ModelRuntime.create({ agentDir: cwd, refreshOnCreate: false });
  modelRuntime.registerNativeProvider(faux.provider);
  const argsFor = (arg) => argv.flatMap((v, i) => v === arg ? [argv[i + 1]] : []);
  const ext = argsFor("--extension").filter((path) => !path.startsWith("builtin:"));
  const flags = new Map(["--swarm-bin", "--swarm-live"].flatMap((flag) =>
    argsFor(flag).map((value) => [flag.slice(2), value])));
  const runtime = await createAgentSessionRuntime(async ({ cwd, agentDir, sessionManager }) => {
    const services = await createAgentSessionServices({
      cwd, agentDir, modelRuntime, settingsManager: SettingsManager.inMemory(),
      extensionFlagValues: flags,
      resourceLoaderOptions: { noExtensions: true, additionalExtensionPaths: ext },
    });
    const result = await createAgentSessionFromServices({ services, sessionManager, model: faux.getModel(model ?? name) });
    assert.deepEqual(result.extensionsResult.errors, []);
    return { ...result, services, diagnostics: services.diagnostics };
  }, { cwd, agentDir: cwd, sessionManager: SessionManager.create(cwd, dir) });
  return { runtime, faux };
}

async function child(argv) {
  assert(argv.includes("--no-extensions"));
  const flag = (name) => argv[argv.indexOf(name) + 1];
  assert.equal(flag("--mode"), "rpc");
  assert.equal(flag("--provider"), "faux");
  const cwd = process.cwd();
  const dir = flag("--session-dir");
  const { runtime: host, faux } = await runtime({ dir, cwd, argv, model: flag("--model") });
  assert(!host.session.getActiveToolNames().includes("subagent"));
  // The fixture marker is written on process start, independent of the model result.
  writeFileSync(join(dir, "pid"), String(process.pid));
  const cmd = 'touch "$FIXTURE_CWD/must-not-exist"';
  switch (process.env.FIXTURE_CASE) {
    case "guard":
      faux.setResponses([call("bash", { command: cmd }, "bash-guard"), (ctx) => {
        const result = ctx.messages.find((m) => m.role === "toolResult");
        assert.equal(result?.isError, true);
        return turn("guard refused");
      }]);
      break;
    case "shutdown":
      // A pending, scripted provider request keeps the child alive until the parent disposes.
      faux.setResponses([() => new Promise(() => {})]);
      break;
    default:
      faux.setResponses([(ctx) => {
        const prompt = ctx.messages.find((m) => m.role === "user");
        return turn("fixture heard " + prompt.content.map((p) => p.text).join("\n").split("\n").at(-1));
      }]);
  }
  await runRpcMode(host);
}

async function fixture(t, scenario, options = {}) {
  const cwd = mkdtempSync(resolve("target/pi-native-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const swarm = join(cwd, "swarm");
  const parentDir = join(cwd, "parent");
  const live = join(cwd, "fixture.live");
  const guards = join(cwd, "guards.json");
  mkdirSync(parentDir);

  writeFileSync(guards, JSON.stringify({ rules: [{ name: "fixture", event: "PreToolUse", tools: ["bash"],
    command: ["/bin/sh", "-c", "printf fixture-denied >&2; exit 2"] }] }));
  const path = process.env.PATH;
  const vars = { SWARM_HOME: swarm, SWARM_AGENT_ID: "orchestrator", SWARM_GUARDS: guards,
    SWARM_PI_NATIVE_CHILD: undefined, HERDR_AGENT_PANE: undefined,
    PI_CODING_AGENT_DIR: cwd, FIXTURE_CASE: scenario, FIXTURE_CWD: cwd, PATH: `${cwd}:${path}` };
  const prior = Object.keys(vars).map((key) => [key, process.env[key]]);
  for (const [key, value] of Object.entries(vars)) {
    if (value === undefined) delete process.env[key]; else process.env[key] = value;
  }

  t.after(() => { for (const [k, v] of prior) if (v === undefined) delete process.env[k]; else process.env[k] = v; });
  const script = fileURLToPath(import.meta.url);
  const quote = (p) => "'" + p.replaceAll("'", "'\\''") + "'";
  writeFileSync(join(cwd, "pi"), `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} --child "$@"\n`, { mode: 0o700 });

  // Ask the actual builder to install Swarm's Pi extension, without starting the probe slot.
  const probe = join(cwd, "probe.jsonl");
  writeFileSync(probe, "{}\n");
  // The chair launch loads composio-hint.ts when it exists, so the fixture holds one.
  mkdirSync(join(cwd, "extensions"), { recursive: true });
  writeFileSync(join(cwd, "extensions", "composio-hint.ts"), "");
  const childArgv = execFileSync(bin, ["_pi-child-argv", "faux", "fixture-haiku", probe, "probe", "1"], { cwd, stdio: "pipe" }).toString();
  // A native child is a leaf, so it loads the built-ins and the live bridge and nothing else.
  const childArgs = JSON.parse(childArgv);
  const loaded = childArgs.flatMap((arg, i) => (childArgs[i - 1] === "--extension" ? [arg] : []));
  assert.deepEqual(loaded, ["builtin:mcp", "builtin:codemode", join(swarm, ".swarm/pi/swarm-live.ts")], "a native child is a leaf");

  const argv = ["--extension", join(swarm, ".swarm/pi/swarm-live.ts"), "--swarm-bin", bin, "--swarm-live", live];
  const { runtime: host, faux } = await runtime({ dir: parentDir, cwd, argv, ...options });
  t.after(() => host.dispose());
  const session = host.session;
  await session.bindExtensions({});
  assert(session.getActiveToolNames().includes("subagent"));
  return { cwd, faux, host, session, live, slot: (id, slot = "1") => join(session.sessionFile.slice(0, -6), "subagents", id, slot) };
}

function output(session) {
  return session.messages.filter((m) => m.role === "toolResult").flatMap((m) => m.content.map((p) => p.text)).join("\n");
}

function waitForFile(dir, file) {
  const path = join(dir, file);
  if (existsSync(path)) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const watcher = watch(dir, () => { if (existsSync(path)) { watcher.close(); clearTimeout(timer); resolve(); } });
    const timer = setTimeout(() => { watcher.close(); reject(Error(`No fixture marker: ${path}`)); }, 5000);
    if (existsSync(path)) { watcher.close(); clearTimeout(timer); resolve(); }
  });
}

if (process.argv[2] === "--child") {
  await child(process.argv.slice(3));
} else {
  test("a foreground child returns its task result and saves a Pi session", async (t) => {
    const { session, faux, slot } = await fixture(t, "foreground");
    faux.setResponses([call("subagent", { task: "one" }, "fg"), turn("parent done")]);
    await session.prompt("Run the scripted fixture");
    assert.match(output(session), /fixture heard one/);
    const dir = slot("fg");
    const log = readdirSync(dir).find((name) => name.endsWith(".jsonl"));
    assert(log);
    assert.match(readFileSync(join(dir, log), "utf8"), /fixture heard one/);
  });

  test("a background batch wakes an idle parent and clears its activity marker", async (t) => {
    const { session, faux, live, slot } = await fixture(t, "background");
    const args = { tasks: [{ task: "one" }, { task: "two", model: "faux/fixture-haiku" }], background: true };
    faux.setResponses([call("subagent", args, "bg"), turn("waiting"), turn("parent woke")]);
    const response = new Promise((resolve) => {
      const unsub = session.subscribe((e) => {
        if (e.type === "message_end" && e.message.content?.some?.((p) => p.text === "parent woke")) { unsub(); resolve(); }
      });
    });
    await session.prompt("Run the scripted fixture");
    assert(existsSync(live.replace(/\.live$/, ".children")));
    await Promise.race([response, new Promise((_, reject) => setTimeout(() => reject(Error("Parent did not wake")), 6000).unref())]);
    assert(!existsSync(live.replace(/\.live$/, ".children")));
    assert(existsSync(join(slot("bg", "2"), "pid")));
    assert(session.messages.some((m) => m.role === "custom" && m.content.includes("fixture heard two")));
  });

  test("a child's Bash command is refused before it can write a file", async (t) => {
    const { session, faux, cwd } = await fixture(t, "guard");
    faux.setResponses([call("subagent", { task: "guard" }, "guard"), turn("parent done")]);
    await session.prompt("Run the scripted fixture");
    assert.match(output(session), /guard refused/);
    assert(!existsSync(join(cwd, "must-not-exist")));
  });

  test("restricted models do not start a child, whether inherited or explicit", async (t) => {
    const { session, faux, slot } = await fixture(t, "models", { name: "fixture-fable-5" });
    faux.setResponses([call("subagent", { task: "blocked" }, "fable"),
      call("subagent", { task: "blocked", model: "faux/fixture-astra-2" }, "astra"), turn("parent done")]);
    await session.prompt("Run the scripted fixture");
    const results = session.messages.filter((m) => m.role === "toolResult");
    assert.equal(results.length, 2);
    assert(results.every((m) => m.isError && m.content[0].text.includes("swarm launch")));
    assert(!existsSync(slot("fable")) && !existsSync(slot("astra")));
  });

  test("closing the parent reaps its running child", async (t) => {
    const { session, faux, host, slot } = await fixture(t, "shutdown");
    faux.setResponses([call("subagent", { task: "stay pending", background: true }, "shutdown"), turn("waiting")]);
    await session.prompt("Run the scripted fixture");
    const dir = slot("shutdown");
    await waitForFile(dir, "pid");
    const pid = Number(readFileSync(join(dir, "pid"), "utf8"));
    assert(process.kill(-pid, 0));
    await host.dispose();
    assert.throws(() => process.kill(-pid, 0), { code: "ESRCH" });
  });
}
