import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import type { App } from "./app";
import { issuePrompt, Lifecycle } from "./lifecycle";

const NEW_WORKSPACE = "/work/proj-worktrees/fix";

/** An App stand-in that records what Lifecycle asks of it. `post` answers by path. */
function fakeApp(post: (path: string, body: Record<string, unknown>) => Promise<unknown>, agents: { id: string; alive: boolean; state?: string }[] = [], importable: { id: string; workspace: string }[] = []) {
  const calls: string[] = [];
  const app = {
    api: {
      post: (path: string, body: Record<string, unknown>) => {
        calls.push(`${path} ${JSON.stringify(body)}`);
        return post(path, body);
      },
      action: async (path: string, body: Record<string, unknown>) => void calls.push(`${path} ${JSON.stringify(body)}`),
      get: async () => ({ chats: importable }),
    },
    choose: async (op: Record<string, unknown>) => (calls.push(`choose ${JSON.stringify(op)}`), true),
    view: { tree: null, workspace: null },
    notice: (text: string) => calls.push(`notice ${text}`),
    refresh: async () => {},
    listsChat: (session: string) => !importable.some((c) => c.id === session),
    selectWorkspace: async (path: string) => void calls.push(`select ${path}`),
    startChat: async (workspace: string, _launch: unknown, draft: string) => void calls.push(`chat ${workspace} ${draft}`),
    openChat: (session: string) => calls.push(`open ${session}`),
    agentsOf: () => ({ info: { id: "chat-1", chair_id: null }, agents }),
    chatTitle: () => "Fix the sidebar",
    leaveChat: (session: string) => calls.push(`leave ${session}`),
    leaveWorkspace: () => {},
  };
  return { life: new Lifecycle(app as unknown as App), calls };
}

describe("reopen", () => {
  it("imports a reopened chat from another adapter, so the sidebar lists it again", async () => {
    const { life, calls } = fakeApp(async () => ({}), [], [{ id: "herdr", workspace: "/w" }]);
    await life.reopen("herdr");
    expect(calls).toEqual(['/api/sessions/unarchive {"ids":["herdr"]}', 'choose {"op":"import_chat","workspace":"/w","chat":"herdr"}', "open herdr"]);
    const own = fakeApp(async () => ({}));
    await own.life.reopen("own");
    expect(own.calls).toEqual(['/api/sessions/unarchive {"ids":["own"]}', "open own"]);
  });
});

describe("new workspace", () => {
  it("offers only Git projects, so a plain folder never opens the dialog", () => {
    const section = (path: string, folder: boolean) => ({ id: path, kind: "project", path, folder, title: path.slice(1), rows: [{ kind: "workspace", id: path }] });
    const { life, calls } = fakeApp(async () => ({}));
    const view = (life as unknown as { app: { view: { tree: unknown; workspace: string | null } } }).app.view;
    view.tree = { sections: [section("/notes", true), section("/repo", false)] };
    view.workspace = "/notes";
    expect(life.projects().map((p) => p.path)).toEqual(["/repo"]);
    life.newWorkspace();
    expect(life.state.newWorkspace).toEqual({ project: "/repo" });
    view.tree = { sections: [section("/notes", true)] };
    life.state = { ...life.state, newWorkspace: null };
    life.newWorkspace();
    expect(life.state.newWorkspace).toBeNull();
    expect(calls).toEqual(["notice New Workspace needs a Git project. Import or create one first."]);
  });

  it("keeps the dialog on a failed setup, with the streamed output and the reply's exit code", async () => {
    let reply!: (value: unknown) => void;
    const { life, calls } = fakeApp(() => new Promise((ok) => (reply = ok)));
    life.state = { ...life.state, newWorkspace: { project: "/work/proj" } };
    const done = life.create({ project: "/work/proj", name: "Fix", mode: "new-branch", base: "main" });
    life.onFrame("setup", { workspace: NEW_WORKSPACE, state: "running" });
    life.onFrame("setup", { workspace: NEW_WORKSPACE, state: "output", text: "boom\n" });
    reply({ workspace: NEW_WORKSPACE, setup: { workspace: NEW_WORKSPACE, state: "failed", exit_code: 9 } });
    await done;
    expect(life.state.newWorkspace).not.toBeNull();
    expect(life.state.create).toMatchObject({ state: "kept", workspace: NEW_WORKSPACE, setup: { output: "boom\n", exitCode: 9 } });
    expect(calls.filter((c) => c.startsWith("select"))).toEqual([]);
    await life.openKept();
    expect(calls).toContain(`select ${NEW_WORKSPACE}`);
    expect(life.state.newWorkspace).toBeNull();
  });

  it("closes on success and starts an empty chat in the new workspace", async () => {
    const { life, calls } = fakeApp(async () => ({ workspace: NEW_WORKSPACE }));
    life.state = { ...life.state, newWorkspace: { project: "/work/proj" } };
    await life.create({ project: "/work/proj", name: "Fix", mode: "new-branch", base: "main" });
    expect(calls.slice(-2)).toEqual([`select ${NEW_WORKSPACE}`, `chat ${NEW_WORKSPACE} `]);
  });

  it("closes on success and starts a chat with the issue's title and body in its draft", async () => {
    const issue = { number: 7, title: "Fix Sidebar", body: "Use less space" };
    const { life, calls } = fakeApp(async () => ({ workspace: NEW_WORKSPACE, issue }));
    await life.create({ project: "/work/proj", name: "", mode: "issue", issue: 7 });
    expect(life.state.create).toBeNull();
    expect(calls.slice(-2)).toEqual([`select ${NEW_WORKSPACE}`, `chat ${NEW_WORKSPACE} Issue #7: Fix Sidebar\n\nUse less space`]);
    expect(issuePrompt({ ...issue, body: "  " })).toBe("Issue #7: Fix Sidebar");
  });

  it("reports a failed setup in a notice when the dialog was closed", async () => {
    const { life, calls } = fakeApp(async () => ({ workspace: NEW_WORKSPACE, setup: { workspace: NEW_WORKSPACE, state: "timeout", exit_code: null } }));
    await life.create({ project: "/work/proj", name: "Fix", mode: "new-branch" });
    expect(life.state.create).toBeNull();
    expect(calls).toContain(`notice The setup script timed out. The workspace was kept at ${NEW_WORKSPACE}.`);
  });

  it("shows the server's refusal and keeps the form", async () => {
    const { life } = fakeApp(async () => {
      throw new ApiError(400, "A folder already exists at /work/proj-worktrees/fix");
    });
    await life.create({ project: "/work/proj", name: "Fix", mode: "new-branch" });
    expect(life.state.create).toEqual({ state: "failed", error: "A folder already exists at /work/proj-worktrees/fix", workspace: null, setup: null });
  });
});

describe("chats", () => {
  it("ends a chat with no live agent at once, and asks first when agents run, children before the chair", async () => {
    const quiet = fakeApp(async () => ({}), [{ id: "orchestrator", alive: false }]);
    quiet.life.endChat("chat-1", true);
    await new Promise((r) => setTimeout(r));
    expect(quiet.life.state.confirm).toBeNull();
    expect(quiet.calls).toContain('/api/sessions/archive {"ids":["chat-1"]}');

    // A chair whose turn is over has an open pane but is not working.
    const idle = fakeApp(async () => ({}), [{ id: "orchestrator", alive: true, state: "done" }]);
    idle.life.endChat("chat-1", true);
    expect(idle.life.state.confirm).toMatchObject({ body: "Archiving closes its agent. The chat moves to Archived." });

    const busy = fakeApp(async () => ({}), [{ id: "orchestrator", alive: true, state: "working" }, { id: "reviewer", alive: true, state: "waiting" }]);
    busy.life.endChat("chat-1", false);
    expect(busy.life.state.confirm).toMatchObject({ title: "End “Fix the sidebar”?", body: "2 agents still working. Ending stops them.", action: "End Chat" });
    expect(await busy.life.confirmed()).toBeNull();
    expect(busy.calls.filter((c) => c.startsWith("/api/agent/close"))).toEqual([
      '/api/agent/close {"session":"chat-1","agent":"reviewer"}',
      '/api/agent/close {"session":"chat-1","agent":"chair"}',
    ]);

    // A group of chats asks once, and archives them in one call.
    const group = fakeApp(async () => ({}), [{ id: "orchestrator", alive: true, state: "done" }]);
    group.life.endChats(["chat-1", "chat-2"], true);
    expect(group.life.state.confirm).toMatchObject({ title: "Archive 2 chats?", body: "Archiving closes their agents. The chats move to Archived.", action: "Archive 2 Chats" });
    expect(await group.life.confirmed()).toBeNull();
    expect(group.calls).toContain('/api/sessions/archive {"ids":["chat-1","chat-2"]}');
    expect(group.calls.filter((c) => c.startsWith("leave"))).toEqual(["leave chat-1", "leave chat-2"]);
  });

  it("marks a refused switch as cancelled only for the server's cancel text", async () => {
    const cancelled = fakeApp(async () => {
      throw new ApiError(409, "model switch cancelled");
    });
    await cancelled.life.switchModel("chat-1", "codex", "gpt-5", null);
    expect(cancelled.life.state.handoffs["chat-1"]).toEqual({ old: "chat-1", phase: "cancelled", canCancel: false, error: null, carry: "summary" });

    const failed = fakeApp(async () => {
      throw new ApiError(409, "Wait for the current reply to finish, or stop it before switching model.");
    });
    await failed.life.switchModel("chat-1", "codex", "gpt-5", null);
    expect(failed.life.state.handoffs["chat-1"]?.phase).toBe("failed");
    expect(failed.life.state.handoffs["chat-1"]?.error).toContain("Wait for the current reply");
  });

  it("keeps the popover and its picks on a resume the server refuses before the pane closes", async () => {
    const app = fakeApp(async () => {
      throw new ApiError(409, "swarm: --effort bogus is not a level of this CLI");
    });
    app.life.openSwitchModel("chat-1");
    await app.life.switchModel("chat-1", "claude", "sonnet", null, "bogus", "resume");
    expect(app.life.state.switchModel).toBe("chat-1");
    expect(app.life.state.handoffs["chat-1"]?.error).toContain("not a level");
    expect(app.calls.filter((c) => c.startsWith("notice"))).toEqual([]);
  });

  it("closes a resume's popover when its pane closes, so the ended-chat dialog never opens", () => {
    const { life } = fakeApp(() => new Promise(() => {}));
    life.openSwitchModel("chat-1");
    void life.switchModel("chat-1", "claude", "sonnet", null, null, "resume");
    life.onFrame("handoff", { old_session: "chat-1", phase: "starting", canCancel: false });
    expect(life.state.switchModel).toBeNull();
  });

  it("runs a switch in each chat at once, and Cancel stops only its own", async () => {
    const { life, calls } = fakeApp(() => new Promise(() => {}));
    void life.switchModel("chat-1", "codex", "gpt-5", null);
    void life.switchModel("chat-2", "claude", "sonnet", null, null, "resume");
    expect(calls.filter((c) => c.startsWith("/api/chat/continue"))).toHaveLength(2);
    // A second switch in the same chat waits for the first.
    void life.switchModel("chat-1", "codex", "gpt-5", null);
    expect(calls.filter((c) => c.startsWith("/api/chat/continue"))).toHaveLength(2);
    void life.cancelSwitch("chat-2");
    expect(calls.at(-1)).toBe('/api/chat/continue/cancel {"old_session":"chat-2"}');
    expect(life.state.handoffs["chat-1"]?.phase).toBe("preparing");
  });

  it("reports a failed switch of a chat whose popover is not open, and does not move to it", async () => {
    let fail: (e: Error) => void = () => {};
    const { life, calls } = fakeApp(() => new Promise((_, reject) => (fail = reject)));
    const run = life.switchModel("chat-1", "codex", "gpt-5", null, null, "resume");
    life.openSwitchModel("chat-2");
    fail(new ApiError(409, "swarm: no session id to resume"));
    await run;
    expect(calls).toContain("notice swarm: no session id to resume");
    expect(life.state.switchModel).toBe("chat-2");
  });

  it("stays on the chat the owner moved to when a resume ends", async () => {
    let done: (v: unknown) => void = () => {};
    const { life, calls } = fakeApp(() => new Promise((r) => (done = r)));
    const app = (life as unknown as { app: { openId: string; refresh: () => Promise<void> } }).app;
    app.openId = "chat-1";
    const run = life.switchModel("chat-1", "claude", "sonnet", null, null, "resume");
    // The owner opens chat-2 while the list reloads, after the server answered.
    app.refresh = async () => void (app.openId = "chat-2");
    done({ session: "chat-1" });
    await run;
    expect(calls).not.toContain("open chat-1");
  });

  it("records the carry, so the progress lists only the steps that run", () => {
    const { life } = fakeApp(() => new Promise(() => {}));
    void life.switchModel("chat-1", "codex", "gpt-5", null, null, "clean");
    expect(life.state.handoffs["chat-1"]?.carry).toBe("clean");
  });

  it("sends the carry and effort of a switch", async () => {
    const app = fakeApp(async () => ({ session: "chat-1" }));
    await app.life.switchModel("chat-1", "pi", "anthropic/claude-sonnet-5-5", null, "high", "resume");
    expect(app.calls).toContain('/api/chat/continue {"old_session":"chat-1","provider":"pi","model":"anthropic/claude-sonnet-5-5","carry":"resume","effort":"high"}');
  });

  it("follows handoff frames for its own chat until a final phase", () => {
    const { life } = fakeApp(() => new Promise(() => {}));
    void life.switchModel("chat-1", "codex", "gpt-5", null);
    life.onFrame("handoff", { old_session: "other-chat", phase: "starting", canCancel: false });
    life.onFrame("handoff", { old_session: "chat-1", phase: "summarizing", canCancel: true });
    expect(life.state.handoffs["chat-1"]).toEqual({ old: "chat-1", phase: "summarizing", canCancel: true, error: null, carry: "summary" });
    life.onFrame("handoff", { old_session: "chat-1", phase: "cancelled", canCancel: false });
    life.onFrame("handoff", { old_session: "chat-1", phase: "starting", canCancel: false });
    expect(life.state.handoffs["chat-1"]?.phase).toBe("cancelled");
  });
});
