import { afterEach, describe, expect, it, vi } from "vitest";
import type { Agent, Session } from "./api";
import { backgroundRows, poll, taskStatus, type BackgroundTask } from "./background";

const session: Session = { id: "chat", cwd: "/work/app", created_at: 0, chair_provider: "claude", adapter: "tmux-swarm", project: null, workspace: "/work/app" };
const agent = (id: string, extra: Partial<Agent> = {}): Agent => ({ id, role: "code.small", state: null, alive: true, pane: `%${id}`, provider: "claude", ...extra });
const task = (id: string, agent: string, status: string): BackgroundTask => ({ id, agent, kind: "shell", title: id, status, output: true });

describe("background rows", () => {
  it("lists the chair first, then live children, each with its tasks, running first", () => {
    const agents = [agent("writer", { created_at: 2 }), agent("gone", { alive: false }), agent("orchestrator", { role: "orchestrator", created_at: 5 })];
    const tasks = [task("b1", "orchestrator", "completed"), task("b2", "orchestrator", "running"), task("b3", "writer", "failed"), task("b4", "gone", "running")];
    expect(backgroundRows(session, agents, tasks).map((r) => [r.key, r.status])).toEqual([
      ["agent:orchestrator", "done"],
      ["task:orchestrator:b2", "working"],
      ["task:orchestrator:b1", "done"],
      ["agent:writer", "done"],
      ["task:writer:b3", "failed"],
    ]);
  });

  it("shows an unknown or a new end state as ended", () => {
    expect(["running", "completed", "killed", "unknown", "paused"].map(taskStatus)).toEqual(["working", "done", "failed", "ended", "ended"]);
  });
});

describe("poll", () => {
  afterEach(() => vi.useRealTimers());

  it("starts a read only after the last one settles, skips hidden ticks, and stops", async () => {
    vi.useFakeTimers();
    let hidden = false;
    let calls = 0;
    let settle = () => {};
    const read = () => {
      calls++;
      return new Promise<void>((done) => (settle = done));
    };
    const stop = poll(read, 1000, () => hidden);
    expect(calls).toBe(1);
    await vi.advanceTimersByTimeAsync(5000);
    expect(calls).toBe(1);
    settle();
    await vi.advanceTimersByTimeAsync(1000);
    expect(calls).toBe(2);
    hidden = true;
    settle();
    await vi.advanceTimersByTimeAsync(3000);
    expect(calls).toBe(2);
    hidden = false;
    await vi.advanceTimersByTimeAsync(1000);
    expect(calls).toBe(3);
    stop();
    settle();
    await vi.advanceTimersByTimeAsync(5000);
    expect(calls).toBe(3);
  });

  it("keeps polling after a failed read", async () => {
    vi.useFakeTimers();
    let calls = 0;
    const stop = poll(() => (calls++, Promise.reject(new Error("down"))), 1000, () => false);
    await vi.advanceTimersByTimeAsync(2000);
    expect(calls).toBe(3);
    stop();
  });
});
