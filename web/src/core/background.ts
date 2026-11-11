// The Cmd+J panel's list: the chat's live agents and their Claude background tasks (ADR 0104).
// Pure, so vitest covers it.

import type { Agent, Session } from "./api";
import { isChair, paneStatus, type PaneStatus } from "./panes";

/** One background shell or subagent, as `/api/background` sends it. */
export type BackgroundTask = {
  id: string;
  agent: string;
  kind: "shell" | "agent";
  title: string;
  /** `running`, `unknown`, or the end state Claude Code reported; new values may appear. */
  status: string;
  summary?: string;
  /** The CLI that owns the task. */
  provider?: "claude" | "codex";
  /** The log's timestamp for the launch. */
  started_at?: string;
  /** True when the task has an output file to show. */
  output: boolean;
};

/** One workspace shell, as `/api/shells` sends it: a shell in the chat's folder that the owner
 *  opened, which lives on after the panel closes. */
export type WorkspaceShell = { id: string; command: string };

export type BackgroundRow =
  | { key: string; type: "agent"; agent: string; title: string; detail: string; status: PaneStatus }
  | { key: string; type: "task"; agent: string; task: BackgroundTask; status: PaneStatus }
  | { key: string; type: "shell"; shell: WorkspaceShell; title: string; status: PaneStatus };

/** A task's status as the glyph every list shows. */
export function taskStatus(status: string): PaneStatus {
  if (status === "running") return "working";
  if (status === "completed") return "done";
  if (status === "failed" || status === "killed") return "failed";
  return "ended";
}

/** The chat-end line for the running tasks (ADR 0104): the newest task's label, how many run, when
 *  the newest started, and the Cmd+J row key that Open picks. `null` when none runs. */
export function runningLine(tasks: readonly BackgroundTask[]) {
  const running = tasks.filter((t) => t.status === "running");
  const newest = running[running.length - 1];
  if (!newest) return null;
  const started = newest.started_at ? Date.parse(newest.started_at) : NaN;
  return {
    label: running.length > 1 ? `${running.length} tasks` : newest.title || newest.id,
    count: running.length,
    startedAt: Number.isNaN(started) ? null : started,
    key: `task:${newest.agent}:${newest.id}`,
  };
}

/** The line's Open asks the open chat to show the Cmd+J panel on this row key. */
export const OPEN_BACKGROUND = "swarm:open-background";

/** `m:ss`, or `h:mm:ss` from an hour. */
export function elapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const pad = (n: number) => String(n).padStart(2, "0");
  return h ? `${h}:${pad(Math.floor(total / 60) % 60)}:${pad(total % 60)}` : `${Math.floor(total / 60)}:${pad(total % 60)}`;
}

/** The chair, then the live children oldest first, each followed by its tasks, running ones first,
 *  then the folder's shells. An ended agent shows nowhere (ADR 0083). */
export function backgroundRows(session: Session | undefined, agents: readonly Agent[] | undefined, tasks: readonly BackgroundTask[], shells: readonly WorkspaceShell[] = []): BackgroundRow[] {
  if (!session || !agents) return [];
  const created = (a: Agent) => (typeof a.created_at === "number" ? a.created_at : Infinity);
  const live = agents
    .filter((a) => paneStatus(a) !== "ended")
    .sort((a, b) => Number(isChair(b, session)) - Number(isChair(a, session)) || created(a) - created(b) || (a.id < b.id ? -1 : 1));
  const rows: BackgroundRow[] = [];
  for (const a of live) {
    const chair = isChair(a, session);
    rows.push({ key: `agent:${a.id}`, type: "agent", agent: a.id, title: chair ? "Chat" : a.id, detail: chair ? a.id : a.role, status: paneStatus(a) });
    const own = tasks.filter((t) => t.agent === a.id);
    for (const t of [...own.filter((t) => t.status === "running"), ...own.filter((t) => t.status !== "running")])
      rows.push({ key: `task:${a.id}:${t.id}`, type: "task", agent: a.id, task: t, status: taskStatus(t.status) });
  }
  // A shell that waits at its prompt is a plain circle; a program in it spins.
  const idle = (command: string) => /^-?(zsh|bash|fish|sh|dash|ksh|tcsh|nu)$/.test(command);
  shells.forEach((shell, i) => rows.push({ key: `shell:${shell.id}`, type: "shell", shell, title: `Shell ${i + 1}`, status: idle(shell.command) ? "ended" : "working" }));
  return rows;
}

/** Calls `read` now, then `ms` after each call settles, so reads never overlap; a tick while
 *  `hidden()` skips the read. A failed read waits for the next tick. Returns a stop. */
export function poll(read: () => Promise<unknown>, ms: number, hidden: () => boolean): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const tick = async () => {
    if (!hidden()) await read().catch(() => {});
    if (!stopped) timer = setTimeout(tick, ms);
  };
  void tick();
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
}

/** Search the current terminal buffer without a provider or a new shell. */
export function terminalMatches(lines: readonly string[], query: string) {
  const q = query.toLowerCase();
  if (!q) return [];
  return lines.flatMap((text, line) => {
    const hits: { line: number; column: number }[] = [];
    for (let column = text.toLowerCase().indexOf(q); column >= 0; column = text.toLowerCase().indexOf(q, column + q.length)) hits.push({ line, column });
    return hits;
  });
}
