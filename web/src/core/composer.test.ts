import { describe, expect, it } from "vitest";
import { ApiError, type Agent, type Api, type Row, type RowPage, type Session, type Snapshot } from "./api";
import { App } from "./app";
import { agentInput, clearPresses, confirmSent, decidePullBack, MISSING_ADAPTER, sends, sentRow, submission, usageLabel } from "./composer";

const user = (id: string, text: string, offset = 10): Row => ({ id, kind: "user", text, order: [offset, 0] });
const queued = (id: string, text: string): Row => ({ id, kind: "queued", text, order: [5, 0] });
const session = (extra: Partial<Session> = {}): Session => ({
  id: "chat",
  cwd: "/work/app",
  created_at: 0,
  chair_provider: "claude",
  adapter: "tmux-solo",
  project: null,
  workspace: "/work/app",
  ...extra,
});
const chair = (extra: Partial<Agent> = {}): Agent => ({ id: "orchestrator", role: "orchestrator", state: null, alive: true, pane: "%1", ...extra });
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

describe("pull-back", () => {
  it("counts two C-u presses per line and one more", () => {
    expect(clearPresses(["one"])).toBe(3);
    expect(clearPresses(["first", "second\nthird"])).toBe(7);
  });

  it("waits while a queued row is still in the rows", () => {
    const fixTypo = queued("q-1", "fix the typo");
    expect(decidePullBack([fixTypo], new Set(["q-1"]), [fixTypo])).toEqual({ state: "waiting" });
  });

  it("pulls a queued text that left without a new user row", () => {
    const fixTypo = queued("q-1", "fix the typo");
    expect(decidePullBack([fixTypo], new Set(["q-1"]), [])).toEqual({ state: "pulled", texts: ["fix the typo"], presses: 3 });
  });

  it("reports already sent when the CLI took the text first, and clears the recalled prompt", () => {
    const fixTypo = queued("q-1", "fix the typo");
    const rows = [user("old-prompt", "fix the typo", 1), user("taken", "fix the typo", 20)];
    expect(decidePullBack([fixTypo], new Set(["q-1", "old-prompt"]), rows)).toEqual({ state: "alreadySent", presses: 3 });
  });

  it("keeps only the texts the CLI did not take", () => {
    const taken = queued("q-1", "run the tests");
    const left = queued("q-2", "then the linter");
    const rows = [user("taken", "run the tests", 20)];
    expect(decidePullBack([taken, left], new Set(["q-1", "q-2"]), rows)).toEqual({ state: "pulled", texts: ["then the linter"], presses: 3 });
  });
});

describe("sent rows", () => {
  it("leave when a new user row with the same text shows, one row for one text", () => {
    const earlier = user("earlier", "again");
    const first = sentRow("again", "codex", [earlier]);
    const second = sentRow("again", "codex", [earlier]);
    expect(first.row.detail).toBe("Sent · joins after the next tool call");
    expect(sentRow("hi", "agy", []).row.detail).toBe("Sent");
    const left = confirmSent([first, second], [earlier, user("logged", "again ")]);
    expect(left).toEqual([second]);
    const none = [first];
    expect(confirmSent(none, [earlier])).toBe(none);
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

  it("pulls back only Claude in tmux, and reads running from the chair state", () => {
    expect(agentInput(session(), [chair({ state: "working" })])).toMatchObject({ disabled: null, running: true, pullBack: true });
    expect(agentInput(session({ adapter: "herdr" }), [chair()]).pullBack).toBe(false);
    expect(agentInput(session({ chair_provider: "codex" }), [chair()]).pullBack).toBe(false);
  });

  it("shows the context left, else the token count", () => {
    expect(usageLabel(null)).toBe("Usage");
    expect(usageLabel({ context: { context_tokens: 41_200 }, remaining_percent: 79 })).toBe("79% left");
    expect(usageLabel({ context: { context_tokens: 41_200 } })).toBe("41K");
  });
});

/** An App on a fake server: rows for one chat, a snapshot, and a log of the action POSTs. */
function harness(fail?: (path: string) => string | null) {
  const posts: { path: string; body: Record<string, unknown> }[] = [];
  const page: RowPage = { page: { start_offset: 0, end_offset: 10 }, rows: [user("hello", "hello", 1)], usage: null };
  const api = {
    rows: async () => page,
    action: async (path: string, body: Record<string, unknown>) => {
      posts.push({ path, body });
      const err = fail?.(path);
      if (err) throw new ApiError(409, err);
    },
  } as unknown as Api;
  const app = new App(api);
  const snapshot = (agent: Partial<Agent> = {}) =>
    (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({ version: 1, sessions: [session()], agents: { chat: [chair(agent)] } });
  return { app, posts, snapshot };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("chat actions", () => {
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

  it("shows a running Codex send as a Sent row until the log has it, and drops it when the turn ends", async () => {
    const { app, snapshot } = harness();
    const codex = (agent: Partial<Agent>) =>
      (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({
        version: 2,
        sessions: [session({ chair_provider: "codex" })],
        agents: { chat: [chair(agent)] },
      });
    snapshot();
    codex({ state: "working" });
    app.openChat("chat");
    await settle();
    await app.send("chat", "also check the docs");
    expect(app.view.lines.map((l) => [l.row.kind, l.row.text, l.row.detail])).toEqual([
      ["user", "hello", undefined],
      ["queued", "also check the docs", "Sent · joins after the next tool call"],
    ]);
    codex({ state: "done" });
    app.openChat("chat");
    expect(app.view.lines.map((l) => l.row.kind)).toEqual(["user"]);
  });
});

describe("pull-back through the app", () => {
  it("presses Up, waits for the queued row to leave, clears the CLI box, and puts the text in the draft", async () => {
    const g = globalThis as unknown as { requestAnimationFrame?: unknown };
    g.requestAnimationFrame = (fn: () => void) => setTimeout(fn, 0);
    try {
      const keys: string[] = [];
      let app!: App;
      const api = {
        rows: async (): Promise<RowPage> => ({ page: { start_offset: 0, end_offset: 10 }, rows: [user("hello", "hello", 1), queued("q-1", "fix the typo")], usage: null }),
        action: async (_path: string, body: Record<string, unknown>) => {
          keys.push(String(body.key));
          // Claude logs a popAll; the server's next rows frame drops the queued row.
          if (body.key === "Up")
            (app as unknown as { onSse(e: { event: string; data: string }): void }).onSse({
              event: "rows",
              data: JSON.stringify({ key: "chat:chair", remove: ["q-1"], end_offset: 20 }),
            });
        },
      } as unknown as Api;
      app = new App(api);
      (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({ version: 1, sessions: [session()], agents: { chat: [chair({ state: "working" })] } });
      app.openChat("chat");
      await settle();
      expect(app.view.composer!.queued).toBe(1);
      app.setDraft("chat", "");
      await app.pullBack("chat");
      expect(keys).toEqual(["Up", "C-u", "C-u", "C-u"]);
      expect(app.draft("chat")).toBe("fix the typo");
      expect(app.view.composer!.state.banner).toBeNull();
    } finally {
      delete g.requestAnimationFrame;
    }
  });
});
