// The live text of a pi turn for the swarm chat (ADR 0131). `swarm launch` loads this file with
// `--extension` and names the file with `--swarm-live`. The file holds `{thinking, text}` of the
// assistant message that still streams, so tool boxes, clocks, and queued messages never reach the
// chat. It is empty between messages, because pi logs a message when it ends.
// pi has no `swarm guard` hook (ADR 0086), so each bash call goes to the `swarm` binary that
// `--swarm-bin` names, for swarm's own walls. Like `swarm guard`, a call it cannot check is blocked.
import { spawnSync } from "node:child_process";
import { renameSync, rmSync, writeFileSync } from "node:fs";
import { stripVTControlCharacters } from "node:util";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

/** How often the file is written while text streams; serve reads it at the same rate. */
const WRITE_MS = 250;

// Pi's native dialogs report OSC 7501 blocked/question without an extension listener.
function registerQuestions(pi: ExtensionAPI) {
  pi.registerTool({
    name: "ask_user_question",
    label: "Ask the owner",
    description: "Ask the owner one question and wait for their answer. Offer choices or omit them for free text.",
    parameters: Type.Object({
      question: Type.String({ minLength: 1 }),
      choices: Type.Optional(Type.Array(Type.String({ minLength: 1 }), { minItems: 2, maxItems: 9 })),
    }),
    executionMode: "sequential",
    async execute(_id, { question, choices }, signal, _onUpdate, ctx) {
      if (!ctx.hasUI) throw new Error("Cannot ask the owner without an interactive UI.");
      const title = dialogText(question);
      const options = choices?.map((choice) => dialogText(choice).replace(/\s+/g, " "));
      if (!title || options?.some((option) => !option) || (options && new Set(options).size !== options.length)) {
        throw new Error("Give a nonempty question and distinct, nonempty choices.");
      }
      // Pi keeps the trailing newline as a blank separator before the choices.
      const answer = options
        ? await ctx.ui.select(`${title}\n`, options, { signal })
        : await ctx.ui.input(title, "", { signal });
      return {
        content: [{ type: "text", text: answer === undefined ? "The owner did not answer." : `Owner answered:\n${answer}` }],
        details: { answer: answer ?? null },
      };
    },
  });
  pi.registerTool({
    name: "approve_plan",
    label: "Request plan approval",
    description: "Show the owner the full proposed plan and wait for a decision. Do not act on the plan unless the owner approves it explicitly.",
    parameters: Type.Object({ plan: Type.String({ minLength: 1, description: "The full plan for the owner to review" }) }),
    executionMode: "sequential",
    async execute(_id, { plan }, signal, _onUpdate, ctx) {
      if (!ctx.hasUI) throw new Error("Cannot request approval without an interactive UI.");
      const body = dialogText(plan);
      if (!body) throw new Error("Give a nonempty plan for the owner to review.");
      // select() renders its title, not a description option. The closing line also marks the
      // plan selector for Swarm when the top of the dialog is above the viewport.
      const title = `Approve this plan?\n\n${body}\n\nReview the plan above before you answer.\n`;
      const choice = await ctx.ui.select(title, ["Request changes", "Approve"], { signal });
      const approved = choice === "Approve" && !signal?.aborted;
      const changes = choice === "Request changes" && !signal?.aborted
        ? (await ctx.ui.input("What should change? Enter a note or press Esc to skip.", "", { signal }))?.trim()
        : undefined;
      let text = approved ? "The owner approved the plan." : choice === "Request changes"
        ? "The owner requested changes. Do not act on this plan."
        : "The owner did not approve the plan. Do not act on it.";
      if (changes) text += `\n\nOwner feedback:\n${changes}`;
      return { content: [{ type: "text", text }], details: { approved } };
    },
  });
}

function dialogText(text: string): string {
  return stripVTControlCharacters(text).replace(/\r\n?/g, "\n").replace(/\t/g, "   ")
    // Keep line breaks but not terminal controls or bidirectional overrides from the model.
    .replace(/[\p{Cc}\u202a-\u202e\u2066-\u2069]/gu, (ch) => ch === "\n" ? ch : "")
    .trim();
}

export default function (pi: ExtensionAPI) {
  registerQuestions(pi);
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
    return { block: true, reason };
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
