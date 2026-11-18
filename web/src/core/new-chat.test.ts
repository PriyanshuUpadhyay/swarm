import { describe, expect, it } from "vitest";
import type { App } from "./app";
import { NewChat, targets } from "./new-chat";
import type { TreeView } from "./nav";

const row = (id: string, startsChatWorktree: boolean) => ({ id, kind: "workspace", newChatEnabled: true, startsChatWorktree });
const tree = {
  sections: [
    { kind: "project", path: "/hub", folder: false, title: "hub", rows: [row("/hub/wt/main", true), row("/hub/wt/fix", false)] },
    { kind: "project", path: "/notes", folder: true, title: "notes", rows: [row("/notes", false)] },
  ],
} as unknown as TreeView;

describe("new chat composer", () => {
  it("starts on a hub's main for a new worktree, and in a plain folder itself", () => {
    expect(targets(tree, "/hub/wt/fix")).toEqual([
      { cwd: "/hub/wt/main", project: "/hub", title: "hub", git: true },
      { cwd: "/notes", project: "/notes", title: "notes", git: false },
    ]);
  });

  it("posts the prompt, place and run, shows the steps, then opens the chat and sends queued text", async () => {
    // A frame can beat the POST reply, and a failed send of the box's text shows a notice.
    const calls: string[] = [];

    const app = {
      navSeq: 1,
      view: { tree, workspace: "/hub/wt/fix", openId: null },
      notice: (text: string) => calls.push(`notice ${text}`),
      api: {
        post: async (path: string, body: unknown) => (calls.push(`${path} ${JSON.stringify(body)}`), { start: "s1" }),
        action: async (path: string, body: { text: string }) => {
          if (body.text === "Then the docs") throw new Error("chair is gone");
          calls.push(`${path} ${JSON.stringify(body)}`);
        },
      },
      actions: { openNew: async (session: string, open: boolean) => void calls.push(`open ${session} ${open}`) },
      composing: { setDraft: (session: string, text: string) => void calls.push(`draft ${session} ${text}`) },
    };
    const chats = new NewChat(app as unknown as App);

    chats.open();
    expect(chats.state.composer).toMatchObject({ cwd: "/hub/wt/main", place: { kind: "new" } });

    const run = { provider: "codex", model: "gpt-5", effort: "high", account: null };
    const posted = chats.start("/hub/wt/main", "Fix the flaky smoke test\n", { kind: "new" }, { role: "chat", run });
    chats.onFrame("chat-start", { start: "s1", step: "worktree", words: "Make the worktree", state: "running" });
    await posted;
    expect(calls[0]).toBe('/api/chat/new {"cwd":"/hub/wt/main","role":"chat","prompt":"Fix the flaky smoke test","place":{"kind":"new"},"provider":"codex","model":"gpt-5","effort":"high"}');
    expect(chats.state.composer).toBeNull();
    expect(chats.shown()).toMatchObject({ id: "s1", title: "Fix the flaky smoke test", from: "/hub/wt/fix" });

    expect(chats.shown()?.steps).toEqual([{ step: "worktree", words: "Make the worktree", state: "running", error: null }]);
    chats.onFrame("chat-start", { start: "s1", step: "worktree", state: "completed", workspace: "/hub/wt/fix-flaky-smoke-test", branch: "swarm/fix-flaky-smoke-test" });
    chats.onFrame("setup", { workspace: "/hub/wt/fix-flaky-smoke-test", state: "running" });
    chats.onFrame("setup", { workspace: "/hub/wt/fix-flaky-smoke-test", state: "output", text: "npm ci\n" });
    chats.setDraft("s1", "Also run the e2e suite");
    chats.queue("s1");
    chats.setDraft("s1", "Then the docs");
    expect(chats.shown()).toMatchObject({ branch: "swarm/fix-flaky-smoke-test", steps: [{ step: "worktree", state: "completed" }], setup: { output: "npm ci\n" } });

    await chats.onFrame("chat-start", { start: "s1", step: "done", state: "completed", session: "chat-9" });
    expect(chats.state.starts).toEqual({});
    expect(calls.slice(1)).toEqual([
      "open chat-9 true",
      '/api/chat/send {"session":"chat-9","agent":"chair","text":"Also run the e2e suite"}',
      "draft chat-9 Then the docs",
      "notice Your next message did not send: chair is gone. It is in the chat's text box.",
    ]);
  });

  it("ends the start's wait after 10 s and keeps the composer, since the server may still start it", async () => {
    const notices: string[] = [];
    let signal: AbortSignal | undefined;

    const app = {
      navSeq: 1,
      view: { tree, workspace: "/hub/wt/fix", openId: null },
      notice: (text: string) => notices.push(text),
      api: {
        post: async (_path: string, _body: unknown, s?: AbortSignal) => {
          signal = s;
          throw new DOMException("timed out", "TimeoutError");
        },
      },
    };
    const chats = new NewChat(app as unknown as App);
    chats.open();

    expect(await chats.start("/hub/wt/main", "Fix it", { kind: "new" }, { role: "chat", run: null })).toBe(false);
    expect(signal).toBeInstanceOf(AbortSignal);
    expect(notices).toEqual(["The server did not answer in 10 s. Check the sidebar for the chat before you press ⌘Enter again."]);
    expect(chats.state.composer).not.toBeNull();
  });

  it("puts a first prompt whose send failed in the chat's text box, and sends nothing after it", async () => {
    const calls: string[] = [];
    const app = {
      navSeq: 1,
      view: { tree, workspace: "/hub/wt/fix", openId: null },
      notice: (text: string) => calls.push(`notice ${text}`),
      api: { post: async () => ({ start: "s1" }), action: async (path: string) => void calls.push(path) },
      actions: { openNew: async (session: string) => void calls.push(`open ${session}`) },
      composing: { setDraft: (session: string, text: string) => void calls.push(`draft ${session} ${text}`) },
    };
    const chats = new NewChat(app as unknown as App);
    await chats.start("/hub/wt/main", "Fix it", { kind: "new" }, { role: "chat", run: null });
    chats.setDraft("s1", "Then the docs");

    chats.onFrame("chat-start", { start: "s1", step: "send", state: "failed", error: "chair is gone" });
    await chats.onFrame("chat-start", { start: "s1", step: "done", state: "completed", session: "chat-9" });
    expect(calls).toEqual(["open chat-9", "draft chat-9 Fix it\n\nThen the docs", "notice Your first prompt did not send: chair is gone. Your text is in the chat's text box."]);
  });

  it("gives a failed start's prompt, queued text and draft to the next composer when it closes", async () => {
    const app = {
      navSeq: 1,
      view: { tree, workspace: "/hub/wt/fix", openId: null },
      notice: () => {},
      api: { post: async () => ({ start: "s1" }) },
    };
    const chats = new NewChat(app as unknown as App);
    await chats.start("/hub/wt/main", "Fix it", { kind: "new" }, { role: "chat", run: null });
    chats.setDraft("s1", "Then the docs");
    chats.queue("s1");
    chats.setDraft("s1", "And the tests");

    await chats.onFrame("chat-start", { start: "s1", step: "done", state: "failed", error: "setup failed" });
    chats.dismiss("s1");
    chats.open();
    expect(chats.composing()?.text).toBe("Fix it\n\nThen the docs\n\nAnd the tests");
  });

  it("starts a failed ⌘T start again with its queued text, as its folder is no composer target", async () => {
    const posts: unknown[] = [];
    const app = {
      navSeq: 1,
      view: { tree, workspace: "/hub/wt/fix", openId: null },
      notice: () => {},
      api: { post: async (_path: string, body: unknown) => (posts.push(body), { start: `s${posts.length}` }) },
    };
    const chats = new NewChat(app as unknown as App);
    await chats.start("/hub/wt/fix", "", { kind: "here" }, { role: "chat", run: null });
    chats.setDraft("s1", "Then the docs");
    chats.queue("s1");

    await chats.onFrame("chat-start", { start: "s1", step: "done", state: "failed", error: "" });
    expect(chats.state.starts.s1.error).toBe("The chat did not start.");
    chats.retry("s1");
    await new Promise((done) => setTimeout(done));
    expect(chats.state.starts.s2.queued).toEqual(["Then the docs"]);
    expect(chats.state.unsent).toBe("");
  });

  it("gives the next composer both a hidden composer's text and a failed start's prompt", async () => {
    const app = {
      navSeq: 1,
      view: { tree, workspace: "/hub/wt/fix", openId: null },
      notice: () => {},
      api: { post: async () => ({ start: "s1" }) },
    };
    const chats = new NewChat(app as unknown as App);
    await chats.start("/hub/wt/main", "Fix it", { kind: "new" }, { role: "chat", run: null });
    await chats.onFrame("chat-start", { start: "s1", step: "done", state: "failed", error: "setup failed" });
    chats.open();
    chats.typed("Draft one");
    app.navSeq = 2;

    chats.dismiss("s1");
    chats.open();
    expect(chats.composing()?.text).toBe("Fix it\n\nDraft one");
  });
});
