// The live text of a pi turn for the swarm chat (ADR 0131). `swarm launch` loads this file with
// `--extension` and names the file with `--swarm-live`. The file holds `{thinking, text}` of the
// assistant message that still streams, so tool boxes, clocks, and queued messages never reach the
// chat. It is empty between messages, because pi logs a message when it ends.
// pi has no `swarm guard` hook (ADR 0086), so each bash call goes to the `swarm` binary that
// `--swarm-bin` names, for swarm's own walls. Like `swarm guard`, a call it cannot check is blocked.
import { spawnSync } from "node:child_process";
import { renameSync, rmSync, writeFileSync } from "node:fs";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** How often the file is written while text streams; serve reads it at the same rate. */
const WRITE_MS = 250;

export default function (pi: ExtensionAPI) {
  pi.registerFlag("swarm-live", {
    type: "string",
    description: "File that swarm reads for the text of the streaming message",
  });
  pi.registerFlag("swarm-bin", {
    type: "string",
    description: "The swarm binary that checks each bash call",
  });
  pi.on("tool_call", async (event) => {
    const bin = pi.getFlag("swarm-bin");
    if (event.toolName !== "bash" || typeof bin !== "string" || bin === "") return;
    const run = spawnSync(bin, ["guard", "pi", "PreToolUse"], {
      input: JSON.stringify({ tool_name: "bash", tool_input: event.input, cwd: process.cwd() }),
      encoding: "utf8",
      timeout: 10_000,
    });
    if (run.status === 0) return;
    const reason = run.stderr?.trim() || `swarm guard could not run (${run.error ?? `exit ${run.status}`}), so the call is blocked`;
    // `INBOX_WAIT` in src/guard.rs: the turn ends here, and the next ring starts a new one.
    return { block: true, reason, terminate: reason.startsWith("swarm guard: end your turn") };
  });
  let written = "";
  let next = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  const file = () => {
    const path = pi.getFlag("swarm-live");
    return typeof path === "string" && path !== "" ? path : undefined;
  };
  // pi awaits each handler, so a write is small, local, and never awaited on a slow reader.
  const write = () => {
    timer = undefined;
    const path = file();
    if (!path || next === written) return;
    try {
      writeFileSync(`${path}.tmp`, next);
      renameSync(`${path}.tmp`, path);
      written = next;
    } catch {
      // A preview that cannot be written must not stop the turn.
    }
  };
  pi.on("message_update", (event) => {
    if (event.message.role !== "assistant") return;
    const thinking: string[] = [];
    const text: string[] = [];
    for (const block of event.message.content) {
      if (block.type === "thinking") thinking.push(block.thinking);
      else if (block.type === "text") text.push(block.text);
    }
    next = JSON.stringify({ thinking: thinking.join("\n\n"), text: text.join("\n\n") });
    timer ??= setTimeout(write, WRITE_MS);
  });
  pi.on("message_end", () => {
    clearTimeout(timer);
    next = "";
    write();
  });
  pi.on("session_shutdown", () => {
    clearTimeout(timer);
    const path = file();
    if (path) rmSync(path, { force: true });
  });
}
