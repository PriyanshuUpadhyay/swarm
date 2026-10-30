// The live text of a pi turn for the swarm chat (ADR 0131). `swarm launch` loads this file with
// `--extension` and names the file with `--swarm-live`. The file holds `{thinking, text}` of the
// assistant message that still streams, so tool boxes, clocks, and queued messages never reach the
// chat. It is empty between messages, because pi logs a message when it ends.
import { renameSync, rmSync, writeFileSync } from "node:fs";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** How often the file is written while text streams; serve reads it at the same rate. */
const WRITE_MS = 250;

export default function (pi: ExtensionAPI) {
  pi.registerFlag("swarm-live", {
    type: "string",
    description: "File that swarm reads for the text of the streaming message",
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
