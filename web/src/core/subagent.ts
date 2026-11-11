// The open subagent chat: a subagent's own log, read-only (ADR 0140).
/** Joins the parent agent and the call id; the server's `subagent::MARK` splits it. */
export const subagentAgent = (agent: string, callId: string) => `${agent}~sub~${callId}`;

export type OpenSubagent = { chatId: string; callId: string; title: string };

let current: OpenSubagent | null = null;
const listeners = new Set<() => void>();

export const openSubagent = (next: OpenSubagent) => set(next);
export const closeSubagent = () => set(null);
export const currentSubagent = () => current;
export const subscribeSubagent = (fn: () => void) => {
  listeners.add(fn);
  return () => void listeners.delete(fn);
};

function set(next: OpenSubagent | null) {
  current = next;
  for (const fn of listeners) fn();
}
