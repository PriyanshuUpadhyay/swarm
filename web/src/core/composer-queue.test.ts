import { describe, expect, it } from "vitest";
import type { Api, Row, RowPage, Snapshot } from "./api";
import { App } from "./app";
import { chair, session } from "./chat-fixtures";
import { harness, settle, user } from "./composer-harness";

const queued = (id: string, text: string): Row => ({ id, kind: "queued", text, order: [5, 0] });

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
  it("moves the app's own Sent texts to the draft on Up and after a stop", async () => {
    let box = "cleared";
    let held = "steer one steer two second line";
    const { app, posts, snapshot } = harness(undefined, (path) =>
      path === "/api/chat/pull-back" ? { outcome: box === "cleared" ? "pulled" : "alreadySent", text: null, held } : path === "/api/chat/interrupt" ? { ok: true, box, held } : null,
    );
    snapshot({ provider: "pi", state: "working" });
    app.openChat("chat");
    await settle();
    expect(app.chair("chat").pullBack).toBe(true);
    await app.send("chat", "steer one");
    await app.send("chat", "steer two\nsecond line");
    expect(app.chats.get("chat")!.sent.length).toBe(2);
    app.setDraft("chat", "and the docs");
    await app.pullBack("chat");
    expect(posts.at(-1)).toEqual({ path: "/api/chat/pull-back", body: { session: "chat", agent: "chair" } });
    expect(app.draft("chat")).toBe("steer one\nsteer two\nsecond line\nand the docs");
    expect(app.chats.get("chat")!.sent.length).toBe(0);

    // A stop: pi puts the queue back in its box, the server empties it, and the draft takes it.
    // pi took "steer three" just before the stop, so only "steer four" was in the box.
    app.setDraft("chat", "");
    await app.send("chat", "steer three");
    await app.send("chat", "steer four");
    held = "steer four";
    await app.interrupt("chat");
    expect(app.draft("chat")).toBe("steer four");
    expect(app.chats.get("chat")!.sent.map((s) => s.row.text)).toEqual(["steer three"]);
    app.chats.get("chat")!.sent = [];
    expect(app.chats.get("chat")!.sent.length).toBe(0);

    // A box that was not emptied keeps its rows, so the text is never in two places.
    box = "unknown";
    app.setDraft("chat", "");
    await app.send("chat", "steer five");
    await app.interrupt("chat");
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
    expect(app.chats.get("chat")!.sent.length).toBe(1);
    expect(app.view.composer!.queued).toBe(0);
    await app.pullBack("chat");
    expect(posts.map((p) => p.path)).not.toContain("/api/chat/pull-back");
  });
});
