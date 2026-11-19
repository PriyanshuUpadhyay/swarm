// The parent hands the task to the child's RPC session only after its Swarm guard is ready.
import { spawn, spawnSync } from "node:child_process";
import { renameSync, rmSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { Type } from "typebox";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

const MAX_CHILDREN = 4;
const MAX_TASK = 16_000;
const START_MS = 30_000;
const CHILD_MS = 5 * 60_000;
const TERM_MS = 500;
const RESULT_CHARS = 8_000;
const READY = "swarm-pi-child-ready-v1";
export const nativeChild = process.env.SWARM_PI_NATIVE_CHILD !== undefined;

export function reportChildReady(pi: ExtensionAPI) {
  pi.on("session_start", (_event, ctx) => {
    const bin = pi.getFlag("swarm-bin");
    if (ctx.mode !== "rpc" || typeof bin !== "string" || !bin || !ctx.model ||
        String(ctx.model.api) === "pi-virtual" ||
        ctx.model.provider !== process.env.SWARM_PI_CHILD_PROVIDER ||
        ctx.model.id !== process.env.SWARM_PI_CHILD_MODEL) {
      ctx.shutdown();
      return;
    }
    ctx.ui.notify(READY, "info");
  });
}

type Result = { slot: number; state: "started" | "done" | "failed"; text: string };
type Child = { done: Promise<Result>; stop: (reason: string) => void };

function start(argv: string[], cwd: string, provider: string, model: string, task: string, slot: number): Child {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    SWARM_PI_NATIVE_CHILD: "1",
    SWARM_PI_CHILD_PROVIDER: provider,
    SWARM_PI_CHILD_MODEL: model,
  };
  for (const name of ["SWARM_SESSION_ID", "SWARM_AGENT_ID", "SWARM_PTY_HOST", "SWARM_PTY_PANE",
    "SWARM_CHILD_HOST", "PI_SESSION_FILE", "PI_SESSION_ID", "TMUX_PANE", "HERDR_PANE_ID",
    "SUPERSET_TERMINAL_ID", "REX_BLOCK"]) delete env[name];
  // detached creates a process group on Unix. Keep the handles and pipes owned by the pane.
  const proc = spawn("pi", argv, { cwd, env, detached: true, stdio: ["pipe", "pipe", "pipe"] });
  let ready = false, settled = false, reason = "", answer = "", modelError = "", stderr = "";
  let escalation: ReturnType<typeof setTimeout> | undefined;
  let pendingKill: Promise<void> | undefined;
  function signal(sig: NodeJS.Signals | 0) {
    if (!proc.pid) return false;
    try { process.kill(-proc.pid, sig); return true; } catch { return false; }
  }
  function stop(why: string) {
    if (reason) return;
    reason = why;
    if (signal("SIGTERM")) {
      pendingKill = new Promise((resolve) => {
        escalation = setTimeout(() => { signal("SIGKILL"); resolve(); }, TERM_MS);
      });
    }
  }
  const startup = setTimeout(() => stop("Child did not load its Swarm guard and pinned model"), START_MS);
  const deadline = setTimeout(() => stop("Child reached its five-minute time limit"), CHILD_MS);
  proc.stdin.on("error", () => stop("Child closed its task input"));
  proc.stderr.setEncoding("utf8").on("data", (data: string) => { stderr = (stderr + data).slice(-RESULT_CHARS); });
  const lines = createInterface({ input: proc.stdout });
  lines.on("line", (line) => {
    if (reason) return;
    let event;
    try { event = JSON.parse(line); } catch { return; }
    if (!ready && event?.type === "extension_error") {
      stop("A child extension failed to load: " + String(event.error).slice(0, RESULT_CHARS));
    } else if (!ready && event?.type === "extension_ui_request" &&
        event.method === "notify" && event.message === READY) {
      ready = true;
      clearTimeout(startup);
      proc.stdin.write(JSON.stringify({ type: "prompt", message: "Delegated task:\n" + task }) + "\n");
    } else if (ready && event?.type === "response" && event.command === "prompt" && !event.success) {
      stop("Child rejected the task: " + String(event.error).slice(0, RESULT_CHARS));
    } else if (ready && event?.type === "message_end" && event.message?.role === "assistant") {
      answer = "";
      for (const part of event.message.content ?? []) {
        if (part.type === "text") answer += String(part.text).slice(0, RESULT_CHARS - answer.length);
      }
      modelError = event.message.stopReason === "error"
        ? String(event.message.errorMessage ?? "Child model failed").slice(0, RESULT_CHARS) : "";
    } else if (ready && event?.type === "agent_settled") {
      settled = !event.aborted;
      proc.stdin.end(); // RPC EOF disposes the child and flushes its session file.
    }
  });
  proc.on("error", (error) => stop(error.message));
  const done = new Promise<Result>((resolve) => {
    proc.once("close", async (code) => {
      clearTimeout(startup);
      clearTimeout(deadline);
      if (!pendingKill || !signal(0)) {
        // On normal exit there can still be descendants in the group.
        clearTimeout(escalation);
        if (signal("SIGTERM")) {
          await new Promise<void>((done) => setTimeout(() => { signal("SIGKILL"); done(); }, TERM_MS));
        }
      } else {
        await pendingKill;
      }
      const failed = reason || modelError || (!settled || code !== 0
        ? stderr.trim() || (ready ? "Child exited before finishing" : "Child did not load its Swarm guard and pinned model")
        : "");
      resolve({ slot, state: failed ? "failed" : "done", text: failed || answer || "Child returned no text" });
    });
  });
  return { done, stop };
}

const Task = Type.Object({
  task: Type.String({ minLength: 1, maxLength: MAX_TASK }),
  model: Type.Optional(Type.String({ description: "Exact provider/model ID; otherwise use the parent's model" })),
});

function selectedModel(model: string | undefined, ctx: ExtensionContext) {
  if (model === undefined) return ctx.model;
  const slash = model.indexOf("/");
  return slash > 0 ? ctx.modelRegistry.find(model.slice(0, slash), model.slice(slash + 1)) : undefined;
}

export function registerSubagents(pi: ExtensionAPI) {
  if (process.env.SWARM_AGENT_ID !== "orchestrator" || process.env.HERDR_AGENT_PANE === "1") return;
  const active = new Set<Child>();
  let closing = false;
  let cancelEpoch = 0;
  const markActivity = () => {
    const live = pi.getFlag("swarm-live");
    if (typeof live !== "string" || !live.endsWith(".live")) return;
    const path = live.slice(0, -5) + ".children";
    if (active.size === 0) rmSync(path, { force: true });
    else {
      writeFileSync(path + ".tmp", String(process.pid));
      renameSync(path + ".tmp", path);
    }
  };
  const cancel = async (reason: string) => {
    ++cancelEpoch;
    const children = [...active];
    children.forEach((child) => child.stop(reason));
    await Promise.all(children.map((child) => child.done));
  };
  pi.on("session_start", () => { closing = false; });
  pi.on("agent_settled", async (event) => {
    if (event.aborted) await cancel("Parent turn aborted");
  });
  pi.on("session_shutdown", async () => {
    closing = true;
    await cancel("Parent session closed");
  });
  pi.registerCommand("swarm-subagents-stop", {
    description: "Stop the native subagents of this pane",
    handler: async () => { await cancel("Stopped in the parent pane"); },
  });

  pi.registerTool({
    name: "subagent",
    label: "Swarm subagent",
    description: "Delegate one task or a batch of independent tasks to leaf Pi workers. Use only if the host and workflow permit native workers. A background run sends a result and wakes this pane when it finishes.",
    parameters: Type.Object({
      task: Type.Optional(Task.properties.task),
      model: Type.Optional(Type.String({ description: "Exact provider/model ID; otherwise use the parent's model" })),
      tasks: Type.Optional(Type.Array(Task, { minItems: 1, maxItems: MAX_CHILDREN })),
      background: Type.Optional(Type.Boolean({ description: "Return immediately and wake the parent when done; default false" })),
    }),
    async execute(callId, params, signal, _update, ctx) {
      if (closing || signal?.aborted) throw new Error("Parent session is stopping");
      if (params.tasks ? params.task !== undefined || params.model !== undefined : !params.task) {
        throw new Error("Pass task and optionally model, or pass tasks with a model per task");
      }
      const tasks = params.tasks ?? [{ task: params.task!, model: params.model }];
      if (!tasks.length || tasks.length + active.size > MAX_CHILDREN) {
        throw new Error(`At most ${MAX_CHILDREN} native Pi children can run in this pane at once`);
      }
      const parent = ctx.sessionManager.getSessionFile();
      const bin = pi.getFlag("swarm-bin");
      if (!parent || typeof bin !== "string" || !bin) throw new Error("Swarm cannot build a guarded child launch for this session");
      // Build every launch before starting any child. No await between the cap check and reserve.
      const plans = tasks.map(({ task, model: requested }, i) => {
        if (!task.trim()) throw new Error("A child task cannot be empty");
        const model = selectedModel(requested, ctx);
        if (!model || String(model.api) === "pi-virtual") {
          throw new Error("Use an exact, non-virtual Pi provider/model ID, or use a visible pane with `swarm launch`");
        }
        const slot = i + 1;
        const run = spawnSync(bin, ["_pi-child-argv", model.provider, model.id, parent, callId, String(slot)], {
          cwd: ctx.cwd, encoding: "utf8", timeout: 2000, maxBuffer: 64 * 1024,
        });
        if (run.status !== 0) throw new Error(run.stderr?.trim() || String(run.error ?? "Swarm could not build the child argv"));
        const argv: unknown = JSON.parse(run.stdout);
        if (!Array.isArray(argv) || !argv.length || !argv.every((arg) => typeof arg === "string")) {
          throw new Error("Swarm returned no valid child argv");
        }
        return { argv, provider: model.provider, model: model.id, task, slot };
      });
      const epoch = cancelEpoch;
      const children = plans.map((p) => start(p.argv, ctx.cwd, p.provider, p.model, p.task, p.slot));
      for (const child of children) active.add(child);
      const abort = () => { void cancel("Parent turn aborted"); };
      signal?.addEventListener("abort", abort, { once: true });
      const done = Promise.all(children.map((child) => child.done.then((result) => {
        active.delete(child);
        try { markActivity(); } catch { /* status is advisory */ }
        return result;
      }))).finally(() => signal?.removeEventListener("abort", abort));
      try { markActivity(); } catch (error) {
        await cancel("Cannot record active Pi children");
        throw error;
      }
      const payload = (results: Result[]) => ({ v: 1, callId, results });
      const content = (results: Result[]) => results.map((r) => `Child ${r.slot} (${r.state}): ${r.text}`).join("\n\n");
      if (!params.background) {
        const results = await done;
        return { content: [{ type: "text", text: content(results) }], details: payload(results) };
      }
      void done.then((results) => {
        if (!closing && epoch === cancelEpoch) {
          pi.sendMessage({ customType: "swarm-pi-subagents", display: true, content: content(results), details: payload(results) },
            { deliverAs: "followUp", triggerTurn: true });
        }
      });
      const results: Result[] = plans.map(({ slot }) => ({ slot, state: "started", text: "The result will arrive in this pane" }));
      return { content: [{ type: "text", text: content(results) }], details: payload(results) };
    },
  });
}
