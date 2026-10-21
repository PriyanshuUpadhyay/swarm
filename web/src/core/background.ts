// The Cmd+J panel's list: the chat's live agents and their Claude background tasks (ADR 0102).
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
  /** True when the task has an output file to show. */
  output: boolean;
};

export type BackgroundRow =
  | { key: string; type: "agent"; agent: string; title: string; detail: string; status: PaneStatus }
  | { key: string; type: "task"; agent: string; task: BackgroundTask; status: PaneStatus };

/** A task's status as the glyph every list shows. */
export function taskStatus(status: string): PaneStatus {
  if (status === "running") return "working";
  if (status === "completed") return "done";
  if (status === "failed" || status === "killed") return "failed";
  return "ended";
}

/** The chair, then the live children oldest first, each followed by its tasks, running ones first.
 *  An ended agent shows nowhere (ADR 0083). */
export function backgroundRows(session: Session | undefined, agents: readonly Agent[] | undefined, tasks: readonly BackgroundTask[]): BackgroundRow[] {
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
  return rows;
}
