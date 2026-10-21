// A chat and its chair, for the composer and run chip tests.

import type { Agent, Session } from "./api";

export const session = (extra: Partial<Session> = {}): Session => ({
  id: "chat",
  cwd: "/work/app",
  created_at: 0,
  chair_provider: "claude",
  adapter: "tmux-swarm",
  project: null,
  workspace: "/work/app",
  ...extra,
});
export const chair = (extra: Partial<Agent> = {}): Agent => ({ id: "orchestrator", role: "orchestrator", state: null, alive: true, pane: "%1", ...extra });
