import { describe, expect, it } from "vitest";
import type { Agent, Api, Row, RowFrame, RowPage, Session, Snapshot, SubscribeKey } from "./api";
import { App, paneId } from "./app";
import { attachCommand, chatWidth, columns, columnWidth, MAX_SPLIT_CHATS, movePane, paneCells, paneStatus, saveSplit, splitShare } from "./panes";

const session = (extra: Partial<Session> = {}): Session => ({
  id: "chat",
  cwd: "/work/app",
  created_at: 0,
  chair_provider: "claude",
  adapter: "tmux",
  project: null,
  workspace: "/work/app",
  ...extra,
});
const agent = (id: string, extra: Partial<Agent> = {}): Agent => ({ id, role: "code.small", state: null, alive: true, pane: `%${id}`, provider: "claude", log: `/logs/${id}.jsonl`, ...extra });
const chair = agent("orchestrator", { role: "orchestrator" });

describe("pane cells", () => {
  it("reads one status per agent: no pane or not alive is ended, and no state is done", () => {
    expect(paneStatus(agent("reviewer", { alive: false, state: "working" }))).toBe("ended");
    // `swarm close` clears the pane, and the listing then says nothing about alive.
    expect(paneStatus(agent("reviewer", { pane: null, alive: null, state: "working" }))).toBe("ended");
    expect(paneStatus(agent("reviewer"))).toBe("done");
    expect(paneStatus(agent("reviewer", { alive: null, state: "waiting" }))).toBe("waiting");
    expect(paneStatus(agent("reviewer", { state: "ended" }))).toBe("done");
  });

  it("lists the children oldest first, then by id, without the chair", () => {
    const agents = [agent("writer", { created_at: 20 }), chair, agent("provider-chair-id"), agent("tester", { created_at: 10 }), agent("reviewer", { created_at: 10 })];
    const cells = paneCells(session({ chair_id: "provider-chair-id" }), agents);
    expect(cells.map((c) => c.id)).toEqual(["reviewer", "tester", "writer"]);
  });

  it("shows only children whose pane is alive", () => {
    const agents = [chair, agent("finished", { alive: false }), agent("closed", { pane: null, alive: null }), agent("live")];
    expect(paneCells(session(), agents).map((c) => [c.id, c.status])).toEqual([["live", "done"]]);
  });

  it("names the model, else the provider, and whether the hooks reported a log", () => {
    const [bare, named] = paneCells(session(), [agent("named", { model: "opus" }), agent("bare", { provider: "codex", log: null })]);
    expect([named.model, named.hasLog]).toEqual(["opus", true]);
    expect([bare.model, bare.hasLog]).toEqual(["codex", false]);
  });
});

describe("strip layout", () => {
  it("starts an odd count with one full-height pane and pairs the rest", () => {
    expect(columns(0)).toEqual([]);
    expect(columns(1)).toEqual([[0]]);
    expect(columns(3)).toEqual([[0], [1, 2]]);
    expect(columns(4)).toEqual([
      [0, 1],
      [2, 3],
    ]);
  });

  it("moves focus by place: the chat is left of the first column, and edges hold", () => {
    expect(movePane(null, 3, "right")).toBe(0);
    expect(movePane(null, 3, "left")).toBeNull();
    expect(movePane(null, 0, "right")).toBeNull();
    expect(movePane(0, 3, "left")).toBeNull();
    expect(movePane(0, 3, "right")).toBe(1);
    expect(movePane(1, 3, "down")).toBe(2);
    expect(movePane(2, 3, "down")).toBe(2);
    expect(movePane(2, 3, "up")).toBe(1);
    expect(movePane(2, 3, "right")).toBe(2);
    // From the lower pane of a column to a one-pane column.
    expect(movePane(2, 3, "left")).toBe(0);
    // The same row in the next two-pane column.
    expect(movePane(1, 4, "right")).toBe(3);
  });

  it("gives the columns the right side and the chat the rest, at least 400, until a drag", () => {
    expect(chatWidth(1180, null, 440)).toBe(740);
    expect(chatWidth(2000, null, 667)).toBe(1333);
    expect(chatWidth(1180, null, 880)).toBe(400);
  });

  it("a drag leaves room for one column", () => {
    expect(chatWidth(1000, 800)).toBe(560);
    expect(chatWidth(1000, 100)).toBe(400);
    expect(chatWidth(700, 650)).toBe(400);
  });

  it("gives a column a third of the area, at least 440, and a drag up to 90%", () => {
    expect(columnWidth(1800)).toBe(600);
    expect(columnWidth(900)).toBe(440);
    expect(columnWidth(1000, 2000)).toBe(900);
    expect(columnWidth(1000, 100)).toBe(440);
  });

  it("keeps a split between 25% and 75%", () => {
    expect(splitShare()).toBe(0.5);
    expect(splitShare(0.1)).toBe(0.25);
    expect(splitShare(0.9)).toBe(0.75);
  });

  it("saves splits per chat, drops columns no longer shown, and resets one", () => {
    let widths = saveSplit({ column: 500 }, "chat-a", ["reviewer", "tester"], "reviewer", 0.3);
    widths = saveSplit(widths, "chat-b", ["reviewer"], "reviewer", 0.6);
    widths = saveSplit(widths, "chat-a", ["tester"], "tester", 0.4);
    expect(widths).toEqual({ column: 500, splits: { "chat-b": { reviewer: 0.6 }, "chat-a": { tester: 0.4 } } });
    expect(saveSplit(widths, "chat-a", ["tester"], "tester", null).splits).toEqual({ "chat-b": { reviewer: 0.6 } });
  });

  it("keeps the splits of the 50 chats saved last", () => {
    let widths = {};
    for (let i = 0; i <= MAX_SPLIT_CHATS; i++) widths = saveSplit(widths, `chat-${i}`, ["reviewer"], "reviewer", 0.3);
    const chats = Object.keys((widths as { splits: object }).splits);
    expect(chats).toHaveLength(MAX_SPLIT_CHATS);
    expect(chats[0]).toBe("chat-1");
  });

  it("quotes an agent id for the shell only when it needs it", () => {
    expect(attachCommand("wp-panes.1")).toBe("swarm attach wp-panes.1");
    expect(attachCommand("it's a test")).toBe(`swarm attach 'it'"'"'s a test'`);
  });
});

/** An App on a fake server with one chat and two children. It logs row requests, action POSTs and
 *  subscriptions. */
function harness(agents: Agent[] = [chair, agent("reviewer", { created_at: 1 }), agent("tester", { created_at: 2 })]) {
  const rowCalls: string[] = [];
  const posts: { path: string; body: Record<string, unknown> }[] = [];
  const subscriptions: string[][] = [];
  const row = (id: string, text: string): Row => ({ id, kind: "assistant", text, order: [1, 0] });
  const api = {
    rows: async (_session: string, who: string): Promise<RowPage> => {
      rowCalls.push(who);
      return { page: { start_offset: 0, end_offset: 10 }, rows: [row(`${who}-first`, `${who} says hello`)], usage: null };
    },
    action: async (path: string, body: Record<string, unknown>) => void posts.push({ path, body }),
    subscribe: async (_stream: string, keys: SubscribeKey[]) => void subscriptions.push(keys.map((k) => k.key).sort()),
  } as unknown as Api;
  const app = new App(api);
  const inner = app as unknown as { applySnapshot(s: Snapshot): void; notify(): void; streamId: string; pending: Map<string, RowFrame[]>; flush(): void };
  inner.streamId = "stream";
  const snapshot = (list = agents, s = session()) => {
    inner.applySnapshot({ version: 1, sessions: [s], agents: { chat: list } });
    inner.notify();
  };
  snapshot();
  const frame = (key: string, id: string, text: string) => {
    inner.pending.set(key, [{ key, upsert: [row(id, text)], end_offset: 20 }]);
    inner.flush();
  };
  return { app, rowCalls, posts, subscriptions, snapshot, frame };
}

const settle = () => new Promise((r) => setTimeout(r, 0));
const texts = (lines: { row: Row }[]) => lines.map((l) => l.row.text);

describe("child columns", () => {
  it("shows one cell per child and loads a column's rows only when it shows", async () => {
    const { app, rowCalls, subscriptions } = harness();
    app.openChat("chat");
    await settle();
    expect(app.view.strip.cells.map((c) => c.id)).toEqual(["reviewer", "tester"]);
    expect(rowCalls).toEqual(["chair"]);
    expect(app.view.strip.panes.get("reviewer")!.chatState).toBe("loading");

    app.setPaneVisible("reviewer", true);
    await settle();
    expect(rowCalls).toEqual(["chair", "reviewer"]);
    expect(texts(app.view.strip.panes.get("reviewer")!.lines)).toEqual(["reviewer says hello"]);
    expect(subscriptions.at(-1)).toEqual(["chat:chair", "chat:reviewer"]);

    // A column that scrolls out of view stops following its log, and keeps its rows.
    app.setPaneVisible("reviewer", false);
    expect(subscriptions.at(-1)).toEqual(["chat:chair"]);
    app.setPaneVisible("reviewer", true);
    await settle();
    expect(rowCalls).toEqual(["chair", "reviewer"]);
    expect(subscriptions.at(-1)).toEqual(["chat:chair", "chat:reviewer"]);
  });

  it("puts a child's stream rows in its own column, not in the chat", async () => {
    const { app, frame } = harness();
    app.openChat("chat");
    app.setPaneVisible("reviewer", true);
    await settle();
    frame("chat:reviewer", "review-note", "found one bug");
    expect(texts(app.view.strip.panes.get("reviewer")!.lines)).toEqual(["reviewer says hello", "found one bug"]);
    expect(texts(app.view.lines)).toEqual(["chair says hello"]);
    // A frame for a column that is not loaded is dropped; it must not land in the chair's chat.
    frame("chat:tester", "test-note", "all green");
    expect(texts(app.view.lines)).toEqual(["chair says hello"]);
  });

  it("waits for the hooks to report a child's log before it loads the column", async () => {
    const quiet = agent("reviewer", { log: null });
    const { app, rowCalls, snapshot } = harness([chair, quiet]);
    app.openChat("chat");
    app.setPaneVisible("reviewer", true);
    await settle();
    expect(app.view.strip.panes.get("reviewer")!.chatState).toBe("none");
    expect(rowCalls).toEqual(["chair"]);
    snapshot([chair, agent("reviewer")]);
    await settle();
    expect(rowCalls).toEqual(["chair", "reviewer"]);
    expect(app.view.strip.panes.get("reviewer")!.chatState).toBe("ready");
  });

  it("types, answers and stops in the child, and closes it by its id", async () => {
    const question = { id: "prompt-hash", question: "Run the tests?", choices: ["Yes", "No"] };
    const { app, posts } = harness([chair, agent("reviewer", { state: "waiting", prompt: question }), agent("tester", { state: "working" })]);
    app.openChat("chat");
    await settle();
    const reviewer = app.view.strip.panes.get("reviewer")!.composer!;
    expect(reviewer.agent).toMatchObject({ who: "reviewer", prompt: question, disabled: "The agent is asking a question. Answer it first." });
    await app.answer(reviewer.id, question, 1);
    await app.send(paneId("chat", "tester"), "check the parser");
    await app.stopPane("tester");
    await app.closePane("tester");
    expect(posts).toEqual([
      { path: "/api/chat/answer", body: { session: "chat", agent: "reviewer", prompt_id: "prompt-hash", choice: 1 } },
      { path: "/api/chat/send", body: { session: "chat", agent: "tester", text: "check the parser" } },
      { path: "/api/chat/interrupt", body: { session: "chat", agent: "tester" } },
      { path: "/api/agent/close", body: { session: "chat", agent: "tester" } },
    ]);
    expect(app.view.strip.panes.get("reviewer")!.composer!.state.answering).toEqual({ prompt: "prompt-hash", choice: 1 });
    // The chair's composer did not change.
    expect(app.view.composer!.state.answering).toBeNull();
  });

  it("says why a waiting child cannot take input", async () => {
    const { app } = harness([chair, agent("asking", { state: "waiting" })]);
    app.openChat("chat");
    await settle();
    const input = (id: string) => app.view.strip.panes.get(id)!.composer!.agent;
    expect(input("asking").disabled).toBe("asking waits on a question the app cannot read. Run `swarm attach asking` in a terminal to answer it.");
  });
});

describe("a new chat", () => {
  it("waits for the chair's log, then loads it", async () => {
    const { app, rowCalls, snapshot } = harness();
    snapshot(undefined, session({ chair_log: null }));
    app.openChat("chat");
    await settle();
    expect(rowCalls).not.toContain("chair");
    expect(app.view.chatState).toBe("none");
    snapshot(undefined, session({ chair_log: "/logs/chair.jsonl" }));
    await settle();
    expect(rowCalls).toContain("chair");
    expect(app.view.chatState).toBe("ready");
  });
});

describe("pane focus and zoom", () => {
  it("moves focus from the chat to the panes and back, and bumps the reveal each time", async () => {
    const { app } = harness();
    app.openChat("chat");
    await settle();
    const before = app.view.strip.revealSeq;
    expect(app.movePaneFocus("right")).toBe(true);
    expect(app.view.strip).toMatchObject({ focused: "reviewer", reveal: "reviewer", revealSeq: before + 1 });
    app.movePaneFocus("down");
    expect(app.view.strip.focused).toBe("tester");
    expect(app.movePaneFocus("left")).toBe(false);
    expect(app.view.strip).toMatchObject({ focused: null, reveal: null, revealSeq: before + 3 });
    // A click focuses a pane and does not scroll the strip.
    app.paneFocused("tester");
    expect(app.view.strip).toMatchObject({ focused: "tester", reveal: null, revealSeq: before + 3 });
  });

  it("zooms the focused pane, follows only that pane, and unzooms on the same key", async () => {
    const { app, subscriptions } = harness();
    app.openChat("chat");
    app.setPaneVisible("reviewer", true);
    app.setPaneVisible("tester", true);
    await settle();
    app.toggleZoom();
    expect(app.view.strip.zoomed).toBeNull();
    app.focusPane("tester");
    app.toggleZoom();
    expect(app.view.strip).toMatchObject({ zoomed: "tester", focused: "tester" });
    expect(subscriptions.at(-1)).toEqual(["chat:chair", "chat:tester"]);
    app.toggleZoom();
    expect(app.view.strip.zoomed).toBeNull();
    expect(subscriptions.at(-1)).toEqual(["chat:chair", "chat:reviewer", "chat:tester"]);
  });

  it("finds in the focused pane, and in the chat when no pane has focus", async () => {
    const { app } = harness();
    const g = globalThis as unknown as { requestAnimationFrame?: unknown; document?: unknown };
    g.requestAnimationFrame = () => 0;
    g.document = { querySelector: () => null };
    try {
      app.openChat("chat");
      app.setPaneVisible("reviewer", true);
      await settle();
      app.paneFocused("reviewer");
      app.openFind();
      app.setFindQuery("hello");
      const pane = app.view.strip.panes.get("reviewer")!;
      expect(pane.find.matches.map((m) => m.id)).toEqual(["reviewer-first"]);
      expect(app.view.find.open).toBe(false);
      app.paneFocused(null);
      app.openFind();
      expect(app.view.find.matches.map((m) => m.id)).toEqual(["chair-first"]);
      expect(app.view.strip.panes.get("reviewer")!.find.open).toBe(false);
    } finally {
      delete g.requestAnimationFrame;
      delete g.document;
    }
  });

  it("frees a chat's columns when another chat opens", async () => {
    const { app, subscriptions } = harness();
    app.openChat("chat");
    app.setPaneVisible("reviewer", true);
    await settle();
    app.focusPane("reviewer");
    app.openChat("other");
    await settle();
    expect(app.view.strip).toMatchObject({ cells: [], focused: null });
    expect(subscriptions.at(-1)).toEqual(["chat:chair", "other:chair"]);
  });
});
