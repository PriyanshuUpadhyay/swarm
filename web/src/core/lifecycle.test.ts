import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import type { App } from "./app";
import { issuePrompt, Lifecycle, setupFailure, setupFrame } from "./lifecycle";

const NEW_WORKSPACE = "/work/proj-worktrees/fix";
const OTHER_WORKSPACE = "/work/proj-worktrees/other";

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

describe("setup frames", () => {
  it("follows the first workspace that starts, appends its output, and ends on its final frame", () => {
    let run = setupFrame(null, { workspace: NEW_WORKSPACE, state: "running" });
    run = setupFrame(run, { workspace: OTHER_WORKSPACE, state: "output", text: "not ours\n" });
    run = setupFrame(run, { workspace: NEW_WORKSPACE, state: "output", channel: "stdout", text: "installing\n" });
    run = setupFrame(run, { workspace: NEW_WORKSPACE, state: "output", channel: "stderr", text: "warn\n" });
    run = setupFrame(run, { workspace: NEW_WORKSPACE, state: "failed", exit_code: 3 });
    expect(run).toEqual({ workspace: NEW_WORKSPACE, state: "failed", output: "installing\nwarn\n", exitCode: 3, error: null });
    expect(setupFailure(run!)).toBe(`The setup script failed with exit code 3. The workspace was kept at ${NEW_WORKSPACE}.`);
  });

  it("ignores output before a run starts, and keeps only the newest output past the cap", () => {
    expect(setupFrame(null, { workspace: NEW_WORKSPACE, state: "output", text: "early" })).toBeNull();
    const run = setupFrame(setupFrame(null, { workspace: NEW_WORKSPACE, state: "running" }), { workspace: NEW_WORKSPACE, state: "output", text: "x".repeat(300 * 1024) + "END" });
    expect(run!.output.length).toBe(256 * 1024);
    expect(run!.output.endsWith("END")).toBe(true);
  });
});

describe("new workspace", () => {
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
    expect(cancelled.life.state.handoff).toEqual({ old: "chat-1", phase: "cancelled", canCancel: false, error: null });

    const failed = fakeApp(async () => {
      throw new ApiError(409, "Wait for the current reply to finish, or stop it before switching model.");
    });
    await failed.life.switchModel("chat-1", "codex", "gpt-5", null);
    expect(failed.life.state.handoff?.phase).toBe("failed");
    expect(failed.life.state.handoff?.error).toContain("Wait for the current reply");
  });

  it("follows handoff frames for its own chat until a final phase", () => {
    const { life } = fakeApp(() => new Promise(() => {}));
    void life.switchModel("chat-1", "codex", "gpt-5", null);
    life.onFrame("handoff", { old_session: "other-chat", phase: "starting", canCancel: false });
    life.onFrame("handoff", { old_session: "chat-1", phase: "summarizing", canCancel: true });
    expect(life.state.handoff).toEqual({ old: "chat-1", phase: "summarizing", canCancel: true, error: null });
    life.onFrame("handoff", { old_session: "chat-1", phase: "cancelled", canCancel: false });
    life.onFrame("handoff", { old_session: "chat-1", phase: "starting", canCancel: false });
    expect(life.state.handoff?.phase).toBe("cancelled");
  });
});
