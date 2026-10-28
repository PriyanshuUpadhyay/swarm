import { describe, expect, it } from "vitest";
import type { Api, Row, RowPage, Snapshot } from "./api";
import { App, paneId } from "./app";
import { chair, session } from "./chat-fixtures";
import { harness, settle, user } from "./composer-harness";

const queued = (id: string, text: string): Row => ({ id, kind: "queued", text, order: [5, 0] });

// A URL longer than pi's box is wide, which wraps it after WRAP_AT characters.
const URL_TAIL = 130;
const WRAP_AT = 75;

describe("pull-back through the app", () => {
  it("asks the server to pull back and puts the returned text before what was typed", async () => {
    const posts: { path: string; body: Record<string, unknown> }[] = [];
    let outcome: { outcome: string; text: string | null } = { outcome: "pulled", text: "fix the typo" };
    const api = {
      rows: async (): Promise<RowPage> => ({ page: { start_offset: 0, end_offset: 10 }, rows: [user("hello", "hello", 1), queued("q-1", "fix the typo")], usage: null }),
      post: async (path: string, body: Record<string, unknown>) => {
        posts.push({ path, body });
        return outcome;
      },
    } as unknown as Api;
    const app = new App(api);
    (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({ version: 1, sessions: [session()], agents: { chat: [chair({ state: "working" })] } });
    app.openChat("chat");
    await settle();
    expect(app.view.composer!.queued).toBe(1);
    app.setDraft("chat", "and the docs");
    await app.pullBack("chat");
    expect(posts).toEqual([{ path: "/api/chat/pull-back", body: { session: "chat", agent: "chair" } }]);
    expect(app.draft("chat")).toBe("fix the typo\nand the docs");
    expect(app.view.composer!.state.banner).toBeNull();

    // A box the server could not confirm keeps the draft and shows why.
    outcome = { outcome: "unconfirmed", text: null };
    await app.pullBack("chat");
    expect(app.draft("chat")).toBe("fix the typo\nand the docs");
    expect(app.view.composer!.state.banner?.text).toMatch(/Could not confirm/);
  });
});

describe("pi's queue back in the draft", () => {
  it("moves the exact typed texts to the draft on Up and after a stop", async () => {
    let box = "cleared";
    let held = "steer one steer two second line";
    const { app, posts, snapshot } = harness(undefined, (path) =>
      path === "/api/chat/pull-back" ? { outcome: box === "cleared" ? "pulled" : "alreadySent", text: null, held } : path === "/api/chat/interrupt" ? { ok: true, box, held } : null,
    );
    // pi's screen cuts and joins the lines, so only the typed texts come back exact.
    snapshot({ provider: "pi", state: "working", queue: [{ mode: "steering", text: "steer one" }, { mode: "steering", text: "steer two second line" }] });
    app.openChat("chat");
    await settle();
    expect(app.chair("chat").pullBack).toBe(true);
    expect(app.view.composer!.queued).toBe(2);
    await app.send("chat", "steer one");
    await app.send("chat", "steer two\nsecond line");
    expect(app.chats.get("chat")!.typed.length).toBe(2);
    app.setDraft("chat", "and the docs");
    await app.pullBack("chat");
    expect(posts.at(-1)).toEqual({ path: "/api/chat/pull-back", body: { session: "chat", agent: "chair" } });
    // The box held only the typed texts, so their exact texts come back, line breaks and all.
    expect(app.draft("chat")).toBe("steer one\nsteer two\nsecond line\nand the docs");
    expect(app.chats.get("chat")!.typed.length).toBe(0);

    // A stop: pi puts the queue back in its box, the server empties it, and the draft takes it.
    // pi took "steer three" just before the stop, so only "steer four" was in the box.
    app.setDraft("chat", "");
    await app.send("chat", "steer three");
    await app.send("chat", "steer four");
    held = "steer four";
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe("steer four");
    expect(app.chats.get("chat")!.typed).toEqual(["steer three"]);
    app.chats.get("chat")!.typed = [];

    // A box that was not emptied keeps its rows, so the text is never in two places.
    box = "unknown";
    app.setDraft("chat", "");
    await app.send("chat", "steer five");
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe("");
  });
});

describe("what pi's box held goes to the draft", () => {
  it("forgets only the typed texts the box held as whole messages, and keeps the owner's own note", async () => {
    let held = "hook";
    const { app, snapshot } = harness(undefined, (path) => (path === "/api/chat/interrupt" ? { ok: true, box: "cleared", held } : null));
    snapshot({ provider: "pi", state: "working" });
    app.openChat("chat");
    await settle();
    // "ok" is inside "hook" but is not a message the box held.
    await app.send("chat", "ok");
    await app.send("chat", "hook");
    await app.interrupt("chat");
    expect(app.chats.get("chat")!.typed).toEqual(["ok"]);
    expect(app.draft("chat")).toBe("hook");

    // A note typed in pi's own box comes back with the queue.
    app.chats.get("chat")!.typed = [];
    app.setDraft("chat", "");
    await app.send("chat", "steer one");
    held = "my note steer one";
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe("my note steer one");
    expect(app.chats.get("chat")!.typed.length).toBe(0);

    // Whitespace alone is nothing to restore.
    app.setDraft("chat", "");
    await app.send("chat", "steer two");
    held = "  \n ";
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe("");
    expect(app.chats.get("chat")!.typed.length).toBe(1);
  });
});

describe("which typed texts pi's box held", () => {
  it("takes the newest texts, so 'ok' taken by pi is not mistaken for part of 'ok then'", async () => {
    const { app, snapshot } = harness(undefined, (path) => (path === "/api/chat/interrupt" ? { ok: true, box: "cleared", held: "ok then" } : null));
    snapshot({ provider: "pi", state: "working" });
    app.openChat("chat");
    await settle();
    await app.send("chat", "ok");
    await app.send("chat", "ok then");
    await app.interrupt("chat");
    // pi took "ok" before the stop. Only "ok then" came back, once.
    expect(app.draft("chat")).toBe("ok then");
    expect(app.chats.get("chat")!.typed).toEqual(["ok"]);
  });

  it("does not take 'ok' for part of what the owner typed in pi's own box", async () => {
    const { app, snapshot } = harness(undefined, (path) => (path === "/api/chat/interrupt" ? { ok: true, box: "cleared", held: "hook" } : null));
    snapshot({ provider: "pi", state: "working" });
    app.openChat("chat");
    await settle();
    await app.send("chat", "ok");
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe("hook");
    expect(app.chats.get("chat")!.typed).toEqual(["ok"]);
  });

  it("holds a wrapped URL and the short message after it together", async () => {
    const url = `https://example.com/${"a".repeat(URL_TAIL)}`;
    const { app, snapshot } = harness(undefined, (path) => (path === "/api/chat/interrupt" ? { ok: true, box: "cleared", held: `${url.slice(0, WRAP_AT)} ${url.slice(WRAP_AT)} y` } : null));
    snapshot({ provider: "pi", state: "working" });
    app.openChat("chat");
    await settle();
    await app.send("chat", url);
    await app.send("chat", "y");
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe(`${url}\ny`);
    expect(app.chats.get("chat")!.typed.length).toBe(0);
  });

  it("matches a long message that pi's box wrapped, by the text without spaces", async () => {
    const url = `https://example.com/${"a".repeat(URL_TAIL)}`;
    const { app, snapshot } = harness(undefined, (path) => (path === "/api/chat/interrupt" ? { ok: true, box: "cleared", held: `${url.slice(0, WRAP_AT)} ${url.slice(WRAP_AT)}` } : null));
    snapshot({ provider: "pi", state: "working" });
    app.openChat("chat");
    await settle();
    await app.send("chat", url);
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe(url);
    expect(app.chats.get("chat")!.typed.length).toBe(0);
  });
});

describe("pi child's queue back in its draft", () => {
  it("moves the child's typed texts to the child's draft on the header Stop", async () => {
    const { app, posts, snapshot } = harness(undefined, (path) => (path === "/api/chat/interrupt" ? { ok: true, box: "cleared", held: "steer one" } : null));
    snapshot({ provider: "claude", state: "done" }, [{ id: "worker", role: "code.routine", state: "working", alive: true, pane: "%2", provider: "pi", log: "/logs/worker.jsonl" }]);
    app.openChat("chat");
    app.setPaneVisible("worker", true);
    await settle();
    const worker = paneId("chat", "worker");
    await app.send(worker, "steer one");
    expect(app.chats.get(worker)!.typed.length).toBe(1);
    await app.stopPane("worker");
    expect(posts.at(-1)).toEqual({ path: "/api/chat/interrupt", body: { session: "chat", agent: "worker" } });
    expect(app.draft(worker)).toBe("steer one");
    expect(app.chats.get(worker)!.typed.length).toBe(0);
    expect(app.draft("chat")).toBe("");
  });
});

describe("a Claude send on its way to the log", () => {
  it("is not a queue that Up pulls back, so Up recalls the prompt", async () => {
    const { app, posts, snapshot } = harness(undefined, () => ({ outcome: "pulled", text: "" }));
    snapshot({ provider: "claude", state: "done" });
    app.openChat("chat");
    await settle();
    await app.send("chat", "recall me");
    expect(app.view.composer!.queued).toBe(0);
    await app.pullBack("chat");
    expect(posts.map((p) => p.path)).not.toContain("/api/chat/pull-back");
  });
});
