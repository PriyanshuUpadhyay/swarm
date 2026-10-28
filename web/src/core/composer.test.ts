import { describe, expect, it, vi } from "vitest";
import { type Agent, type Api, type RowFrame, type RowPage, type Snapshot } from "./api";
import { App } from "./app";
import { chair, session } from "./chat-fixtures";
import { harness, settle, user } from "./composer-harness";
import { agentInput, canBranch, MISSING_ADAPTER, queueRows, sends, submission, usageLabel } from "./composer";

const waitingPrompt = { id: "prompt-hash", question: "Do you want to proceed?", choices: ["Yes", "No"] };

describe("send key", () => {
  const press = (mods: Partial<Record<"metaKey" | "shiftKey" | "altKey" | "ctrlKey", boolean>> = {}) => ({ metaKey: false, shiftKey: false, altKey: false, ctrlKey: false, ...mods });

  it("sends on a bare Return by default, and on ⌘Return only when the setting says so", () => {
    expect(sends(press(), "return")).toBe(true);
    expect(sends(press({ shiftKey: true }), "return")).toBe(false);
    expect(sends(press({ metaKey: true }), "return")).toBe(false);
    expect(sends(press({ metaKey: true }), "commandReturn")).toBe(true);
    expect(sends(press(), "commandReturn")).toBe(false);
    expect(sends(press({ metaKey: true, shiftKey: true }), "commandReturn")).toBe(false);
  });
});

describe("submission", () => {
  it("drops control characters that a CLI reads as keys, and turns CR into LF", () => {
    expect(submission("stop\x1b here\r\nnext\rlast\tend\x7f")).toBe("stop here\nnext\nlast\tend");
  });

  it("adds a space after a last word a CLI popup would take Enter for", () => {
    expect(submission("/clear")).toBe("/clear ");
    expect(submission("read @src/main.rs")).toBe("read @src/main.rs ");
    expect(submission("use $skill")).toBe("use $skill ");
    expect(submission("note:")).toBe("note: ");
    expect(submission("a path\\")).toBe("a path\\ ");
    expect(submission("plain words")).toBe("plain words");
  });
});

describe("queue rows", () => {
  it("show each held message with the CLI's own word for when it joins", () => {
    const rows = queueRows([
      { mode: "steering", text: "first" },
      { mode: "follow-up", text: "second" },
    ]);
    expect(rows.map((r) => [r.kind, r.text, r.detail])).toEqual([
      ["queued", "first", "Steering"],
      ["queued", "second", "Follow-up"],
    ]);
  });
});

describe("chair input", () => {
  it("names why the composer cannot send", () => {
    expect(agentInput(session({ adapter: null }), [chair()]).disabled).toBe(MISSING_ADAPTER);
    expect(agentInput(session(), []).disabled).toBe("The chair has no pane to receive input.");
    const closed = agentInput(session(), [chair({ alive: false })]);
    expect(closed.disabled).toBe("This chat's pane has closed. Start a new chat or switch model.");
    expect(closed.unreachable).toBe(true);
    const asking = agentInput(session(), [chair({ state: "waiting", prompt: waitingPrompt })]);
    expect(asking.disabled).toBe("The agent is asking a question. Answer it first.");
    expect(asking.unreachable).toBe(false);
    expect(asking.prompt).toEqual(waitingPrompt);
    expect(agentInput(session(), [chair({ state: "waiting" })]).disabled).toBe("The agent is asking a question. Answer it first in its pane.");
  });

  it("offers Branch only on an idle chair of a CLI that can branch", () => {
    const input = (extra: Parameters<typeof chair>[0]) => agentInput(session(), [chair({ provider: "claude", state: "done", ...extra })]);
    expect(canBranch(input({}))).toBe(true);
    expect(canBranch(input({ provider: "pi" }))).toBe(true);
    expect(canBranch(input({ provider: "codex" }))).toBe(false);
    expect(canBranch(input({ state: "working" }))).toBe(false);
    expect(canBranch(agentInput(session(), [chair({ provider: "claude" }), chair({ id: "coder", provider: "claude" })], "coder"))).toBe(false);
  });

  it("offers Resume only for an ended chair with a session id", () => {
    const ended = [chair({ alive: false, pane: null })];
    expect(agentInput(session({ chair_id: "id" }), ended).resumable).toBe(true);
    expect(agentInput(session({ chair_id: "id", adapter: null, chair_provider: "codex" }), []).resumable).toBe(true);
    expect(agentInput(session(), ended).resumable).toBe(false);
    expect(agentInput(session({ chair_id: "id", chair_provider: "pi" }), [chair({ alive: false, provider: "pi" })]).resumable).toBe(true);
    expect(agentInput(session({ chair_id: "id" }), [chair()]).resumable).toBe(false);
    expect(agentInput(session({ chair_id: "id" }), ended, "coder").resumable).toBe(false);
  });

  it("pulls back only Claude in tmux, and reads running from the chair state", () => {
    expect(agentInput(session(), [chair({ state: "working" })])).toMatchObject({ disabled: null, running: true, pullBack: true });
    expect(agentInput(session({ adapter: "herdr" }), [chair()]).pullBack).toBe(false);
    expect(agentInput(session({ chair_provider: "codex" }), [chair()]).pullBack).toBe(false);
  });

  it("shows the context left, else the token count", () => {
    expect(usageLabel(null)).toBe("Context");
    expect(usageLabel({ context: { context_tokens: 41_200 }, remaining_percent: 79 })).toBe("Context 79% left");
    expect(usageLabel({ context: { context_tokens: 41_200 } })).toBe("Context 41K");
  });
});

describe("draft skill picks", () => {
  it("keeps each chat's picks with its draft across a switch and a reload", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => saved.set(key, value),
      removeItem: (key: string) => saved.delete(key),
    });
    try {
      const { app } = harness();
      const picks = [{ start: 4, insert: "/skill:plan" }];
      app.setDraft("pi", "fix /skill:plan now", picks);
      app.setDraft("other", "plain words");
      expect(app.draftPicks("other")).toEqual([]);
      expect(app.draftPicks("pi")).toEqual(picks);
      const reloaded = harness().app;
      expect(reloaded.draft("pi")).toBe("fix /skill:plan now");
      expect(reloaded.draftPicks("pi")).toEqual(picks);
      reloaded.setDraft("pi", "fix /skill:plans now");
      expect(reloaded.draftPicks("pi")).toEqual([]);
      expect(harness().app.draftPicks("pi")).toEqual([]);
      reloaded.setDraft("pi", "fix /skill:plan now", picks);
      reloaded.setDraft("pi", "");
      expect(harness().app.draftPicks("pi")).toEqual([]);
      saved.set("composer.draft.legacy", "typed /skill:plan");
      saved.set("composer.skills.legacy", "invalid json");
      expect(harness().app.draft("legacy")).toBe("typed /skill:plan");
      expect(harness().app.draftPicks("legacy")).toEqual([]);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("chat actions", () => {
  it("keeps picked text on failure, clears it after Retry, and protects a later edit", async () => {
    let refuse = true;
    const { app, posts, snapshot } = harness(() => refuse ? "send failed" : null);
    snapshot();
    app.openChat("chat");
    await settle();
    const text = "fix /skill:plan now";
    const picks = [{ start: 4, insert: "/skill:plan" }];
    app.setDraft("chat", text, picks);
    expect(await app.send("chat", "/skill:plan fix now", text)).toBe(false);
    expect(app.draftPicks("chat")).toEqual(picks);
    refuse = false;
    app.view.composer!.state.banner!.retry();
    await settle();
    expect(posts.at(-1)!.body.text).toBe("/skill:plan fix now");
    expect(app.draft("chat")).toBe("");
    expect(app.draftPicks("chat")).toEqual([]);
    app.setDraft("chat", "a later draft");
    await app.send("chat", "/skill:plan fix now", text);
    expect(app.draft("chat")).toBe("a later draft");
  });

  it("sends the cleaned text, then clears the draft only if it still holds that text", async () => {
    const { app, posts, snapshot } = harness();
    snapshot();
    app.openChat("chat");
    await settle();
    app.setDraft("chat", "/clear");
    await app.send("chat", "/clear");
    expect(posts).toEqual([{ path: "/api/chat/send", body: { session: "chat", agent: "chair", text: "/clear " } }]);
    expect(app.draft("chat")).toBe("");
    expect(app.view.composer!.state.draftRev).toBe(1);

    app.setDraft("chat", "typed after the send");
    await app.send("chat", "earlier text");
    expect(app.draft("chat")).toBe("typed after the send");
  });

  it("shows the server's error in the banner, keeps the draft, and sends again only on Retry", async () => {
    let refuse = true;
    const { app, posts, snapshot } = harness(() => (refuse ? "swarm: this chat's pane has closed; start a new chat or switch model" : null));
    snapshot();
    app.openChat("chat");
    await settle();
    app.setDraft("chat", "hello there");
    await app.send("chat", "hello there");
    const banner = app.view.composer!.state.banner!;
    expect(banner.text).toBe("swarm: this chat's pane has closed; start a new chat or switch model");
    expect(app.draft("chat")).toBe("hello there");
    await settle();
    expect(posts).toHaveLength(1);

    refuse = false;
    banner.retry();
    await settle();
    expect(posts).toHaveLength(2);
    expect(app.view.composer!.state.banner).toBeNull();
    expect(app.draft("chat")).toBe("");
  });

  it("sends a pi text as it is; only the composer moves a skill", async () => {
    const { app, posts, snapshot } = harness();
    snapshot({ provider: "pi" });
    app.openChat("chat");
    await settle();
    await app.send("chat", "fix /skill:plan now");
    expect(posts).toEqual([{ path: "/api/chat/send", body: { session: "chat", agent: "chair", text: "fix /skill:plan now" } }]);
  });

  it("does not send while the chair asks a question", async () => {
    const { app, posts, snapshot } = harness();
    snapshot({ state: "waiting", prompt: waitingPrompt });
    app.openChat("chat");
    await settle();
    await app.send("chat", "yes please");
    expect(posts).toEqual([]);
  });

  it("interrupts, and answers with the prompt id and the zero-based choice", async () => {
    const { app, posts, snapshot } = harness();
    snapshot({ state: "waiting", prompt: waitingPrompt });
    app.openChat("chat");
    await settle();
    await app.interrupt("chat");
    await app.answer("chat", waitingPrompt, 1);
    await app.answer("chat", waitingPrompt, 0);
    expect(posts).toEqual([
      { path: "/api/chat/interrupt", body: { session: "chat", agent: "chair" } },
      { path: "/api/chat/answer", body: { session: "chat", agent: "chair", prompt_id: "prompt-hash", choice: 1 } },
    ]);
    expect(app.view.composer!.state.answering).toEqual({ prompt: "prompt-hash", choice: 1 });
  });

  it("shows a Codex send only when the log has it, because Codex shows no queue that swarm reads", async () => {
    const { app, snapshot } = harness();
    snapshot();
    (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({
      version: 2,
      sessions: [session({ chair_provider: "codex" })],
      agents: { chat: [chair({ state: "working" })] },
    });
    app.openChat("chat");
    await settle();
    await app.send("chat", "also check the docs");
    expect(app.view.lines.map((l) => [l.row.kind, l.row.text])).toEqual([
      ["user", "hello"],
      ["notice", "Working…"],
    ]);
  });

  it("shows pi's queue as its screen lists it, and nothing once the screen lists none", async () => {
    const { app, snapshot } = harness();
    const pi = (agent: Partial<Agent>) =>
      (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({
        version: 2,
        sessions: [session({ chair_provider: "pi" })],
        agents: { chat: [chair(agent)] },
      });
    snapshot();
    // The incident: /compact runs, and pi holds the note typed during it.
    pi({ state: "working" });
    app.openChat("chat");
    await settle();
    await app.send("chat", "/compact");
    await app.send("chat", "note typed during compaction");
    const lines = () => app.view.lines.map((l) => [l.row.kind, l.row.text, l.row.detail]);
    expect(lines()).toEqual([
      ["user", "hello", undefined],
      ["notice", "Working…", undefined],
    ]);
    pi({ state: "working", queue: [{ mode: "steering", text: "note typed during compaction" }] });
    app.openChat("chat");
    expect(lines()).toEqual([
      ["user", "hello", undefined],
      ["queued", "note typed during compaction", "Steering"],
      ["notice", "Working…", undefined],
    ]);
    expect(app.view.composer!.queued).toBe(1);
    // pi took the note straight after the compaction, with no idle pane between.
    pi({ state: "working", queue: [] });
    app.openChat("chat");
    expect(lines()).toEqual([
      ["user", "hello", undefined],
      ["notice", "Working…", undefined],
    ]);
    expect(app.view.composer!.queued).toBe(0);
  });

  it("shows Claude's /compact only as a Working line until Claude logs it", async () => {
    const { app, snapshot } = harness();
    snapshot();
    (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({
      version: 2,
      sessions: [session({ chair_provider: "claude" })],
      agents: { chat: [chair({ state: "working" })] },
    });
    app.openChat("chat");
    await settle();
    await app.send("chat", "/compact");
    expect(app.view.lines.map((l) => [l.row.kind, l.row.text])).toEqual([
      ["user", "hello"],
      ["notice", "Working…"],
    ]);
  });
});

describe("usage from the rows stream", () => {
  it("takes a frame's usage, keeps it when a frame has none, and clears it on null", async () => {
    const api = { rows: async (): Promise<RowPage> => ({ page: { start_offset: 0, end_offset: 10 }, rows: [user("hello", "hello", 1)], usage: null }) } as unknown as Api;
    const app = new App(api);
    const inner = app as unknown as { applySnapshot(s: Snapshot): void; pending: Map<string, RowFrame[]>; flush(): void };
    inner.applySnapshot({ version: 1, sessions: [session()], agents: { chat: [chair()] } });
    app.openChat("chat");
    await settle();
    const frame = (extra: Partial<RowFrame>) => {
      inner.pending.set("chat:chair", [{ key: "chat:chair", end_offset: 20, ...extra }]);
      inner.flush();
    };
    frame({ usage: { context: { context_tokens: 41_200 }, remaining_percent: 20 } });
    expect(usageLabel(app.view.composer!.usage)).toBe("Context 20% left");
    frame({ upsert: [user("more", "more", 15)] });
    expect(usageLabel(app.view.composer!.usage)).toBe("Context 20% left");
    frame({ usage: null });
    expect(usageLabel(app.view.composer!.usage)).toBe("Context");
  });
});
