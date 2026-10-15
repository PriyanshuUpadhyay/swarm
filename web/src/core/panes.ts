// Agent pane strip rules ported from the SwiftUI app (AgentStatus, SwarmPanePolicy, PaneStripLayout,
// SwarmAgentID.attachCommand). Pure, so vitest covers them.

import type { Agent, Session } from "./api";

/** The one status every surface shows for an agent. */
export type PaneStatus = "working" | "waiting" | "done" | "failed" | "ended";

/** No pane, or a pane that is not alive, is ended. An alive agent with no known state is idle, so done. */
export function paneStatus(agent: Agent): PaneStatus {
  if (!agent.pane || agent.alive === false) return "ended";
  const s = agent.state;
  return s === "working" || s === "waiting" || s === "failed" ? s : "done";
}

/** A waiting agent paused on a question inside its turn. */
export const midTurn = (status: PaneStatus) => status === "working" || status === "waiting";

export type PaneCell = { id: string; role: string; model: string; status: PaneStatus; provider: string | null; hasLog: boolean };

export const isChair = (agent: Agent, session: Session) => agent.id === "orchestrator" || agent.id === session.chair_id;

/** The chat's child agents whose pane is alive, oldest first. An ended child shows nowhere (ADR 0083). */
export function paneCells(session: Session | undefined, agents: readonly Agent[] | undefined): PaneCell[] {
  if (!session || !agents) return [];
  const created = (a: Agent) => (typeof a.created_at === "number" ? a.created_at : Infinity);
  return agents
    .filter((a) => !isChair(a, session) && paneStatus(a) !== "ended")
    .sort((a, b) => created(a) - created(b) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    .map((a) => {
      const provider = (a.provider as string | null | undefined) ?? null;
      return { id: a.id, role: a.role, model: (a.model as string | null | undefined) ?? provider ?? "unknown", status: paneStatus(a), provider, hasLog: !!a.log };
    });
}

/** Pane indexes per column. An odd count starts with one full-height pane; the rest pair up (ADR 0030). */
export function columns(count: number): number[][] {
  const out: number[][] = count % 2 ? [[0]] : [];
  for (let i = count % 2; i < count; i += 2) out.push([i, i + 1]);
  return out;
}

export type Direction = "left" | "right" | "up" | "down";

/** Spatial focus over pane indexes; null is the chat page, left of the first column. Left and right
 *  cross columns, up and down stay in one. At an edge the focus stays where it is. */
export function movePane(from: number | null, count: number, direction: Direction): number | null {
  const cols = columns(count);
  const col = from === null ? -1 : cols.findIndex((c) => c.includes(from));
  if (from === null || col < 0) return direction === "right" && cols.length ? cols[0][0] : null;
  const row = cols[col].indexOf(from);
  const at = (c: number) => cols[c][Math.min(row, cols[c].length - 1)];
  switch (direction) {
    case "left":
      return col === 0 ? null : at(col - 1);
    case "right":
      return col + 1 < cols.length ? at(col + 1) : from;
    case "up":
      return cols[col][Math.max(row - 1, 0)];
    case "down":
      return cols[col][Math.min(row + 1, cols[col].length - 1)];
  }
}

/** About 60 terminal columns at 12 px. */
export const MIN_COLUMN = 440;
export const MIN_CHAT = 400;

/** With no drag the columns, `columns` px in all, take the right side and the chat page the rest, at
 *  least 400 px; with no column width given it is 90% of the main area. A drag leaves 440 px for the
 *  first column; in a narrow area the 400 px chat minimum wins and the strip scrolls. */
export function chatWidth(main: number, preferred?: number | null, columns?: number): number {
  if (preferred == null) return columns ? Math.max(MIN_CHAT, main - columns) : main * 0.9;
  return Math.min(Math.max(preferred, MIN_CHAT), Math.max(MIN_CHAT, main - MIN_COLUMN));
}

/** The dragged width of every column, kept between 440 px and 90% of the main area (ADR 0026).
 *  With no drag a column is a third of the main area. */
export function columnWidth(main: number, preferred?: number | null): number {
  if (preferred == null) return Math.max(main / 3, MIN_COLUMN);
  return Math.max(Math.min(preferred, main * 0.9), MIN_COLUMN);
}

/** The top pane's share of a two-pane column, kept between 25% and 75%. */
export const splitShare = (preferred?: number | null) => Math.min(Math.max(preferred ?? 0.5, 0.25), 0.75);

/** Saved layout. Splits belong to a chat and are keyed by the column's first agent id. */
export type PaneWidths = { column?: number; chat?: number; splits?: Record<string, Record<string, number>> };

export const MAX_SPLIT_CHATS = 50;

/** Sets or resets one split. Only columns still shown keep theirs; the chat saved last goes to the
 *  end, and the chat saved longest ago goes when more than 50 are kept. */
export function saveSplit(widths: PaneWidths, chat: string, shown: readonly string[], column: string, share: number | null): PaneWidths {
  const { [chat]: old = {}, ...others } = widths.splits ?? {};
  const mine = Object.fromEntries(Object.entries({ ...old, [column]: share }).filter((e): e is [string, number] => e[1] !== null && shown.includes(e[0])));
  const all = Object.entries(Object.keys(mine).length ? { ...others, [chat]: mine } : others);
  return { ...widths, splits: Object.fromEntries(all.slice(-MAX_SPLIT_CHATS)) };
}

/** `swarm attach <id>`, with the id quoted for a shell when it needs it. */
export function attachCommand(id: string): string {
  return `swarm attach ${/^[A-Za-z0-9_.-]+$/.test(id) ? id : `'${id.replaceAll("'", `'"'"'`)}'`}`;
}

/** What an empty column says before the child's hooks report its log. */
export function waitingMessage(provider: string | null): string {
  return provider === "codex" || provider === "agy"
    ? "No chat yet. If this stays, set up swarm's hooks in Settings, so Codex and AGY report their chats."
    : "No chat yet. The agent reports it on its first turn.";
}
