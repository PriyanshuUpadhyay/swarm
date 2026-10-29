import { describe, expect, it, vi } from "vitest";
import { ApiError, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { type ChoiceOp, type SidebarRow, type Tab, type Tabs, type TreeView } from "./nav";

const workspace = "/r/app";
const tab = (session: string): Tab => ({
  id: session, session, title: session, status: null, badge: null, canClose: true, canHide: true,
  pending: null, fields: [], children: null,
});

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

function harness(options: { live?: boolean; beforeClose?: () => Promise<void>; beforeArchive?: () => Promise<void>; beforeTabs?: () => Promise<void>; refuse?: boolean; failArchive?: boolean } = {}) {
  const writes: ChoiceOp[] = [];
  const actions: string[] = [];
  const notices: string[] = [];
  let revision = 1;
  let tabs: Tabs = { workspace, tabs: [tab("older"), tab("next"), tab("open")],
    history: [], selectedChat: "open", previousRecent: null, nextRecent: null, stepTargets: {} };
  const row: SidebarRow = {
    id: workspace, kind: "workspace", depth: 0, parentId: null, expanded: true, hasChildren: false,
    childrenSummary: null, title: "app", detail: "", status: null, counts: [], age: null, help: "app",
    pinned: false, archived: false, missing: false, newChatEnabled: true, dimmed: false, fields: [],
    selection: { workspace, chat: null, agentSession: null, agent: null },
  };
  const tree = (): TreeView => ({
    version: 1, revision: `rev-${revision}`, view: "workspaces", selectedRow: null, archivedStatus: null,
    choicesError: null, prefs: {},
    sections: [{ id: "/r", kind: "project", path: "/r", folder: false, collapseId: "project:/r", expanded: true,
      title: "r", status: null, fields: [], rows: [row, ...tabs.tabs.map((t) => ({
        ...row, id: `chat:${t.session}`, kind: "chat" as const, selection: { ...row.selection!, chat: t.session },
      }))] }],
  });
  const snapshot: Snapshot = {
    version: 1, sessions: ["older", "next", "open"].map((id) => ({ id, cwd: workspace, workspace, project: "/r", created_at: 1, chair_provider: null, adapter: null })),
    agents: options.live ? { open: [{ id: "orchestrator", role: "o", state: "working", alive: true }] } : {},
  };
  const remove = (session: string) => {
    const remaining = tabs.tabs.filter((t) => t.session !== session);
    tabs = { ...tabs, tabs: remaining, selectedChat: tabs.selectedChat === session ? remaining.at(-1)?.session ?? null : tabs.selectedChat };
  };
  const api = {
    snapshot: async () => snapshot,
    tree: async () => tree(),
    tabs: async () => {
      await options.beforeTabs?.();
      return { version: revision, revision: `rev-${revision}`, view: tabs };
    },
    rows: async () => ({ page: { start_offset: 0, end_offset: 0 }, rows: [], usage: null }),
    choice: async (_revision: string, op: ChoiceOp) => {
      writes.push(op);
      if (op.op === "close_tab") {
        await options.beforeClose?.();
        if (options.refuse) throw new ApiError(409, "choices revision is stale");
        if (!tabs.tabs.some((t) => t.id === op.chat)) throw new ApiError(400, "tab is not open");
        remove(op.chat);
      }
      if (op.op === "selected_chat") tabs = { ...tabs, selectedChat: op.chat };
      revision++;
      return { revision: `rev-${revision}`, version: revision };
    },
    action: async (path: string) => {
      actions.push(path);
      if (path === "/api/sessions/archive") {
        await options.beforeArchive?.();
        if (options.failArchive) throw new ApiError(500, "archive failed");
        remove("open");
      }
    },
  } as unknown as Api;
  const app = new App(api);
  app.workspace = workspace;
  app.openId = "open";
  app.tabs = tabs;
  app.tree = tree();
  app.snapshot = snapshot;
  app.restored = true;
  app.revision = "rev-1";
  app.notice = (text) => notices.push(text);
  return { app, writes, actions, notices };
}

describe("closing tabs", () => {
  it("runs two fast closes in order and finds the newly open tab for the second press", async () => {
    const first = deferred();
    const { app, writes, notices } = harness({ beforeClose: () => first.promise });
    const closing = [app.closeTab(), app.closeTab()];
    await vi.waitFor(() => expect(writes.filter((op) => op.op === "close_tab")).toHaveLength(1));
    first.resolve();
    await Promise.all(closing);
    expect(writes.flatMap((op) => op.op === "close_tab" ? [op.chat] : [])).toEqual(["open", "next"]);
    expect(app.openId).toBe("older");
    expect(app.closedTabs).toEqual(["open", "next"]);
    expect(notices).toEqual([]);
  });

  it("lets a later close run after a strip read never settles", async () => {
    vi.useFakeTimers();
    try {
      const { app, writes } = harness({ beforeTabs: () => new Promise(() => {}) });
      const settled = vi.fn();
      const closing = Promise.all([app.closeTab(), app.closeTab("next")]).then(settled);
      await vi.advanceTimersByTimeAsync(4999);
      expect(writes.filter((op) => op.op === "close_tab")).toHaveLength(1);
      expect(settled).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(1);
      expect(writes.flatMap((op) => op.op === "close_tab" ? [op.chat] : [])).toEqual(["open", "next"]);
      expect(settled).toHaveBeenCalledOnce();
      await closing;
    } finally {
      vi.useRealTimers();
    }
  });

  it("leaves the nav sequence unchanged when a background tab closes", async () => {
    const { app } = harness();
    const seq = app.navSeq;
    await app.closeTab("older");
    expect(app.navSeq).toBe(seq);
    expect(app.openId).toBe("open");
    expect(app.closedTabs).toEqual(["older"]);
  });

  it("does not add a refused close to the reopen list", async () => {
    const { app } = harness({ refuse: true });
    await app.closeTab();
    expect(app.closedTabs).toEqual([]);
    expect(app.openId).toBe("open");
  });

  it.each(["confirm", "cancel"])("keeps a replacement dialog and settles its close after %s", async (action) => {
    const archive = deferred();
    const { app, actions } = harness({ live: true, beforeArchive: () => archive.promise });
    const firstClose = app.life.endChat("open", true);
    const confirming = app.life.confirmed();
    await vi.waitFor(() => expect(actions).toContain("/api/sessions/archive"));
    const settled = vi.fn();
    const secondClose = app.life.endChat("open", true).then(settled);
    const replacement = app.life.state.confirm;
    archive.resolve();
    expect(await confirming).toBeNull();
    await firstClose;
    expect(app.life.state.confirm).toBe(replacement);
    expect(settled).not.toHaveBeenCalled();
    if (action === "confirm") expect(await app.life.confirmed()).toBeNull();
    else app.life.close("confirm");
    await vi.waitFor(() => expect(settled).toHaveBeenCalledOnce());
    await secondClose;
    expect(app.life.state.confirm).toBeNull();
  });

  it("puts an archived chat after older closed tabs and offers Undo only after success", async () => {
    const { app } = harness();
    app.closedTabs = ["older"];
    app.notice = vi.fn();
    await app.life.endChat("open", true);
    expect(app.closedTabs).toEqual(["older", "open"]);
    expect(app.notice).toHaveBeenCalledWith("Chat archived.", false, expect.objectContaining({ label: "Undo", run: expect.any(Function) }));
    app.sidebar.unarchive = vi.fn();
    app.navigation.reopenTab();
    expect(app.sidebar.unarchive).toHaveBeenCalledWith("open");
  });

  it("does not offer Undo or change the reopen list when archive fails", async () => {
    const { app } = harness({ failArchive: true });
    app.closedTabs = ["older"];
    app.notice = vi.fn();
    await app.life.endChat("open", true);
    expect(app.closedTabs).toEqual(["older"]);
    expect(app.notice).not.toHaveBeenCalledWith("Chat archived.", false, expect.anything());
  });

});
