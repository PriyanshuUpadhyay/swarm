// Composer rules ported from the SwiftUI app (ComposerState, ComposerQueue, QueuedMessages,
// SwarmSessionInteraction, ChatUsage). Pure, so vitest covers them.

import type { Agent, Row, Session } from "./api";
import type { Meter } from "./settings";

/** `swarm agents --json` `prompt`: a question on the agent's screen (ADR 0029). */
export type Prompt = { id: string; question: string; choices: string[]; details?: string[] };

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
  effort: string | null;
  /** The swarm account; null for the CLI's own login. */
  account: string | null;
  /** Why the composer cannot send now, or null. */
  disabled: string | null;
  /** True when no pane takes input at all; ⌘. and the prompt card need one. */
  unreachable: boolean;
  /** An unreachable chair whose own CLI session can start again in this chat (ADR 0107). */
  resumable: boolean;
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
    effort: (agent?.effort as string | undefined) ?? null,
    account: (agent?.account as string | undefined) ?? null,
    disabled,
    unreachable,
    resumable: unreachable && !child && provider !== null && typeof session?.chair_id === "string",
    running: agent?.state === "working",
    prompt: unreachable ? null : prompt,
    pullBack: provider === "claude" && (adapter === "tmux" || adapter === "tmux-swarm"),
  };
}

/** `/api/rows` `usage` (view::ChatUsage). */
export type Usage = { context?: { context_tokens?: number | null } | null; remaining_percent?: number | null } | null;

const compact = (n: number) => new Intl.NumberFormat("en-US", { notation: "compact" }).format(n);

/** The composer's context line (ChatUsage.contextMeter). */
export function usageLabel(usage: Usage): string {
  const tokens = usage?.context?.context_tokens;
  if (tokens == null) return "Context";
  if (usage?.remaining_percent != null) return `Context ${usage.remaining_percent}% left`;
  return `Context ${compact(tokens)}`;
}

/** The context line's tooltip, which says what its number measures. */
export function usageTitle(usage: Usage): string {
  const tokens = usage?.context?.context_tokens;
  if (tokens == null) return "The context of this chat is not known yet.";
  if (usage?.remaining_percent != null) return `Context of this chat: ${usage.remaining_percent}% of the window is left.`;
  return `Context of this chat: ${compact(tokens)} tokens.`;
}

const CLI_NAMES: Record<string, string> = { claude: "Claude", codex: "Codex", agy: "AGY", pi: "Pi" };

/** The CLI's name in the chip, the popover's groups, and the outcome line. */
export function cliName(id: string): string {
  return Object.hasOwn(CLI_NAMES, id) ? CLI_NAMES[id] : id;
}

/** The run chip: `account · model · thinking`. Pi and AGY have no swarm accounts, so their CLI
 *  name stands in. */
export function runLabel(input: AgentInput): string {
  return [input.account ?? (input.provider ? cliName(input.provider) : null), input.model ?? "Default model", input.effort].filter(Boolean).join(" · ");
}

const LEVEL_WORDS: Record<string, string> = { minimal: "Minimal", low: "Low", medium: "Medium", high: "High", xhigh: "Extra high", max: "Max", ultra: "Ultra" };

/** A thinking level in plain words; an id the app does not know shows as the CLI wrote it. */
export function levelWord(id: string): string {
  return Object.hasOwn(LEVEL_WORDS, id) ? LEVEL_WORDS[id] : id;
}

/** The chip's tooltip: the account and its email when known, then the model and level. */
export function runTip(run: Run, email: string | null = null): string {
  const who = run.account ? `${cliName(run.provider)} account: ${run.account}${email ? ` (${email})` : ""}` : `${cliName(run.provider)} login`;
  return `${who}\n${[run.model ?? "Default model", run.effort && levelWord(run.effort)].filter(Boolean).join(" · ")}`;
}

// Eight hues at one lightness and chroma, so each dot reads the same on both themes.
const ACCOUNT_HUES = [250, 150, 30, 300, 80, 200, 0, 120];

/** One color per account name, the same on every load. */
export function accountColor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.codePointAt(0)!) >>> 0;
  return `oklch(0.68 0.14 ${ACCOUNT_HUES[h % ACCOUNT_HUES.length]})`;
}

/** A meter window's short name: 5h, wk, or the window id. A named Codex limit (`<id>:<window>`)
 *  keeps its id in front, so two limits of one account do not read the same. */
function windowName(m: Meter): string {
  // Claude's Fable week ("fb", src/usage/claude.rs) is a week as long as the plain one.
  const limit = m.window === "fb" ? "Fable " : m.window?.includes(":") ? `${m.window.split(":")[0]} ` : "";
  if (m.window_minutes === 10080 || m.window === "7d") return `${limit}wk`;
  if (m.window_minutes && m.window_minutes % 60 === 0 && m.window_minutes < 1440) return `${limit}${m.window_minutes / 60}h`;
  return m.window ?? m.label;
}

/** What is left in each limit window of one account ("5h 62% · wk 40%"), and the lowest for its
 *  bar. With no meter read, the account list's summary stands in. */
export function limitText(meters: readonly Meter[], provider: string, account: string, summary: string | null): { text: string; left: number | null } {
  const mine = meters.filter((m) => m.provider === provider && m.account === account && m.used_pct !== null);
  if (!mine.length) return { text: summary ?? "", left: null };
  const left = mine.map((m) => 100 - m.used_pct!);
  return { text: mine.map((m, i) => `${windowName(m)} ${left[i]}%`).join(" · "), left: Math.min(...left) };
}

/** One agent run: what the popover picks. */
export type Run = { provider: string; model: string | null; effort: string | null; account: string | null };

export type Carry = "summary" | "clean";

/** What `/api/chat/continue` does with the old chat. */
export type SwitchCarry = Carry | "resume";

export type Outcome = "none" | "resume" | "handoff";

/** The two answers to "what does a switch to another CLI bring", in the popover, the ended-chat
 *  dialog, and Settings. */
export const CARRY_OPTIONS: [Carry, string][] = [
  ["summary", "Summary"],
  ["clean", "Start clean"],
];

/** The saved answer to "what does a switch to another CLI bring"; unset means Summary. */
export function savedCarry(prefs: { switchCarry?: string } | undefined): Carry {
  return prefs?.switchCarry === "clean" ? "clean" : "summary";
}

/** What Apply does (ADR 0110). Inside one CLI the session starts again on the new run with its
 *  history; another CLI, or another Codex account, cannot take the session, so a new chat starts. */
export function switchOutcome(current: Run, draft: Run): Outcome {
  if (current.provider !== draft.provider) return "handoff";
  if (current.model === draft.model && current.effort === draft.effort && current.account === draft.account) return "none";
  // Each Codex session lives in one account home.
  return current.provider === "codex" && current.account !== draft.account ? "handoff" : "resume";
}

const REREAD = "The next reply re-reads the whole chat once, so it costs more.";

/** The What happens line under the popover (design A). `label` names the draft's model. */
export function outcomeText(outcome: Outcome, current: Run, draft: Run, carry: Carry, label: string): string {
  if (outcome === "none") return "Pick a change to see what will happen.";
  const target = [label, draft.effort && levelWord(draft.effort)].filter(Boolean).join(" · ");
  return outcome === "handoff" ? handoffLine(draft, target, carry) : outcome === "resume" ? resumeLine(current, draft, target) : (outcome satisfies never);
}

function handoffLine(draft: Run, target: string, carry: Carry): string {
  return `Starts a new ${cliName(draft.provider)} chat on ${target} with ${carry === "summary" ? "a summary of" : "nothing from"} this one. This chat stays as it is.`;
}

function resumeLine(current: Run, draft: Run, target: string): string {
  if (current.account !== draft.account) {
    const on = current.model !== draft.model || current.effort !== draft.effort ? ` on ${target}` : "";
    const who = draft.account ?? "the default account";
    return `Restarts this chat${on} under ${who}. History stays. Usage now counts against ${who}. The next reply may re-read the chat once.`;
  }
  if (current.model !== draft.model) return `Restarts this chat on ${target}. History stays. ${REREAD}`;
  return `Restarts this chat on ${draft.effort ? levelWord(draft.effort) : "the default"} thinking. History stays. ${REREAD}`;
}

/** Apply's label says what it does. */
export function applyLabel(outcome: Outcome): string {
  switch (outcome) {
    case "none":
      return "Apply";
    case "handoff":
      return "Start new chat";
    case "resume":
      return "Restart and apply";
  }
}

/** Why Apply cannot run now, or null. A switch blocks only its own chat. */
export function applyBlock(chair: AgentInput, draft: Run): string | null {
  if (!chair.provider) return "This chat does not record its CLI, so it cannot switch.";
  if (chair.running) return "Wait for this turn to end, or stop it.";
  return draft.model ? null : "Pick a model.";
}

/** The levels a model can run on: its own list within the CLI's, or the CLI's. */
export function modelLevels(cli: string[], model: string[] | undefined): string[] {
  return model ? model.filter((level) => cli.includes(level)) : cli;
}

/** The draft after a pick of model `m` of CLI `cli`. The draft keeps its level when the model
 *  takes it, else the model's default, else the CLI's, and its account while the CLI stays; back on the chat's own CLI, the chat's
 *  account comes back. */
export function chooseModel(current: Run, draft: Run, cli: { id: string; efforts: string[]; default_effort: string }, m: { id: string; efforts?: string[]; default_effort?: string } | string): Run {
  const model = typeof m === "string" ? m : m.id;
  // launch takes only the CLI's own levels, also when the model lists more.
  const allowed = modelLevels(cli.efforts, typeof m === "string" ? undefined : m.efforts);
  const own = typeof m === "string" ? undefined : m.default_effort;
  const effort = [draft.effort, own, cli.default_effort].find((l) => l && allowed.includes(l)) ?? allowed[0] ?? null;
  const account = cli.id === draft.provider ? draft.account : cli.id === current.provider ? current.account : null;
  return { provider: cli.id, model, effort, account };
}
