// Composer rules ported from the SwiftUI app (ComposerState, ComposerQueue, QueuedMessages,
// SwarmSessionInteraction, ChatUsage). Pure, so vitest covers them.

import type { Agent, Row, Session } from "./api";

/** `swarm agents --json` `prompt`: a question on the agent's screen (ADR 0029). */
export type Prompt = { id: string; question: string; choices: string[] };

export type SendKey = "return" | "commandReturn";

/** Only a bare Return, or ⌘Return when the setting says so, sends; any other Return adds a line. */
export function sends(e: Pick<KeyboardEvent, "metaKey" | "shiftKey" | "altKey" | "ctrlKey">, key: SendKey): boolean {
  return !e.shiftKey && !e.altKey && !e.ctrlKey && e.metaKey === (key === "commandReturn");
}

/** The text that leaves the box, or null when nothing should be sent. */
export function outgoing(draft: string): string | null {
  const text = draft.trim();
  return text ? text : null;
}

/** The text to type into a CLI, so the Enter after it can only submit (Composer.submission).
 *  A typed ESC interrupts the turn, so C0 controls other than TAB and LF go, and CR and CRLF become
 *  LF. A last word ending in `\` or `:`, or starting with `@`, `$` or `/`, gets one space, so a CLI
 *  popup does not take the Enter. */
export function submission(text: string): string {
  // eslint-disable-next-line no-control-regex
  const cleaned = text.replace(/\r\n?/g, "\n").replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, "");
  if (!cleaned || /\s$/.test(cleaned)) return cleaned;
  const word = cleaned.split(/\s+/).pop()!;
  return word.endsWith("\\") || word.endsWith(":") || "@$/".includes(word[0]) ? cleaned + " " : cleaned;
}

/** A typed text that the log does not show yet. Codex, AGY, and pi log nothing until the agent
 *  takes it, and Claude logs a slash command such as /compact only when the command ends.
 *  `started` is true once the agent was seen running after the send. */
export type Sent = { row: Row; before: ReadonlySet<string>; started: boolean };

let sentSeq = 0;

export function sentRow(text: string, provider: string | null, rows: readonly Row[], running = false): Sent {
  const n = ++sentSeq;
  return {
    row: {
      id: `sent-${n}`,
      kind: "queued",
      text: text.trim(),
      detail: provider === "codex" ? "Sent · joins after the next tool call" : "Sent",
      order: [Number.MAX_SAFE_INTEGER, n],
    },
    before: new Set(rows.map((r) => r.id)),
    started: running,
  };
}

/** A turn that ran after the send and then ended took or dropped the text
 *  (ComposerSentMessages.update). Returns the same array when nothing changed. */
export function settleSent(sent: Sent[], running: boolean): Sent[] {
  if (!sent.length) return sent;
  if (running) return sent.every((s) => s.started) ? sent : sent.map((s) => ({ ...s, started: true }));
  const left = sent.filter((s) => !s.started);
  return left.length === sent.length ? sent : left;
}

/** The text a row shows the owner typed. A pi skill message is a command row of name and words.
 *  A pasted `<skill>` block does not match, so its Sent row waits for the turn's end. */
const shown = (r: Row) => (r.kind === "user" || r.kind === "queued" ? r.text.trim() : r.command ? `${r.command.name} ${r.command.arguments}`.trim() : null);

/** Drops each sent text that a new user row now shows; one row confirms one text. Returns the same
 *  array when nothing left. */
export function confirmSent(sent: Sent[], rows: readonly Row[]): Sent[] {
  if (!sent.length) return sent;
  const used = new Set<string>();
  const left = sent.filter((s) => {
    const match = rows.find((r) => !s.before.has(r.id) && !used.has(r.id) && shown(r) === s.row.text);
    if (match) used.add(match.id);
    return !match;
  });
  return left.length === sent.length ? sent : left;
}

/** What the composer can do for one agent of a chat, from the snapshot. */
export type AgentInput = {
  /** The child agent's id; null for the chair. */
  who: string | null;
  provider: string | null;
  model: string | null;
  /** Why the composer cannot send now, or null. */
  disabled: string | null;
  /** True when no pane takes input at all; ⌘. and the prompt card need one. */
  unreachable: boolean;
  running: boolean;
  prompt: Prompt | null;
  /** Claude in tmux keeps a queue that Up pulls back (ADR 0038). */
  pullBack: boolean;
};

export const MISSING_ADAPTER = "This session does not record an adapter, so Swarm cannot reach its panes.";

/** The chair's input, or the input of the child agent `child` (ChildColumnView). */
export function agentInput(session: Session | undefined, agents: readonly Agent[] | undefined, child: string | null = null): AgentInput {
  const agent = agents?.find((a) => a.id === (child ?? "orchestrator"));
  const provider = (agent?.provider as string | null | undefined) ?? (child ? null : session?.chair_provider) ?? null;
  const prompt = (agent?.prompt as Prompt | null | undefined) ?? null;
  const adapter = session?.adapter?.trim() || null;
  let disabled: string | null = null;
  // A child with no live pane has no column (ADR 0083), so only the chair's pane is checked here.
  if (!adapter) disabled = MISSING_ADAPTER;
  else if (!child && !agent?.pane) disabled = "The chair has no pane to receive input.";
  else if (!child && agent?.alive === false) disabled = "This chat's pane has closed. Start a new chat or switch model.";
  const unreachable = disabled !== null;
  // A waiting agent has a dialog open, where Enter means yes, so typed text could approve a tool.
  if (!disabled && agent?.state === "waiting")
    disabled = prompt
      ? "The agent is asking a question. Answer it first."
      : child
        ? `${child} waits on a question the app cannot read. Run \`swarm attach ${child}\` in a terminal to answer it.`
        : "The agent is asking a question. Answer it first in its pane.";
  return {
    who: child,
    provider,
    model: (agent?.model as string | undefined) ?? null,
    disabled,
    unreachable,
    running: agent?.state === "working",
    prompt: unreachable ? null : prompt,
    pullBack: provider === "claude" && (adapter === "tmux" || adapter === "tmux-swarm"),
  };
}

/** `/api/rows` `usage` (view::ChatUsage). */
export type Usage = { context?: { context_tokens?: number | null } | null; remaining_percent?: number | null } | null;

/** The composer's context line (ChatUsage.contextMeter). */
export function usageLabel(usage: Usage): string {
  const tokens = usage?.context?.context_tokens;
  if (tokens == null) return "Usage";
  if (usage?.remaining_percent != null) return `${usage.remaining_percent}% left`;
  return new Intl.NumberFormat("en-US", { notation: "compact" }).format(tokens);
}

/** The model, else the provider's name. A chat on the CLI's default model reports none. */
export function modelLabel(input: AgentInput): string {
  if (input.model) return input.model;
  const p = input.provider;
  return p ? p[0].toUpperCase() + p.slice(1) : "Agent";
}
