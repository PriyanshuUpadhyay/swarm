import { describe, expect, it } from "vitest";
import { RunQueue, type PendingSwitch } from "./run-queue";

describe("run queue", () => {
  it("starts a change queued in a turn once, when the turn ends, and a cancelled one never", () => {
    const queue = new RunQueue();
    const started: string[] = [];
    let working = true;
    const run = () => queue.run(() => (working ? "wait" : "start"), (old, c) => started.push(`${old} ${c.model}`), () => {});
    const change: PendingSwitch = { provider: "claude", model: "sonnet", account: null, effort: "high", carry: "resume" };
    queue.add("chat-1", change);
    run();
    expect(started).toEqual([]);
    working = false;
    run();
    run();
    expect(started).toEqual(["chat-1 sonnet"]);
    expect(queue.state).toEqual({});
    working = true;
    queue.add("chat-2", { ...change, model: "gpt-5" });
    queue.cancel("chat-2");
    working = false;
    run();
    expect(started).toEqual(["chat-1 sonnet"]);
  });

  it("drops a change whose chat can no longer take it, and says so once", () => {
    const queue = new RunQueue();
    const started: string[] = [];
    const dropped: string[] = [];
    queue.add("chat-1", { provider: "claude", model: "sonnet", account: null, effort: null, carry: "resume" });
    const run = () => queue.run(() => "drop", (old) => started.push(old), (old, c) => dropped.push(`${old} ${c.model}`));
    run();
    run();
    expect([started, dropped, queue.state]).toEqual([[], ["chat-1 sonnet"], {}]);
  });
});
