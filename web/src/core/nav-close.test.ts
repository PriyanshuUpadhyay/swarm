import { describe, expect, it, vi } from "vitest";
import { ApiError, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { type ChoiceOp, type SidebarRow, type Tab, type Tabs, type TreeView } from "./nav";

const workspace = "/r/app";
const tab = (session: string, tabOnly = false): Tab => ({
  id: session, session, title: session, status: null, badge: null, canClose: true, canHide: true,
  pending: null, fields: [], children: null, tabOnly,
});

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

function harness(options: { tabOnly?: boolean; live?: boolean; beforeClose?: () => Promise<void>; beforeArchive?: () => Promise<void>; beforeTabs?: () => Promise<void>; refuse?: boolean; failArchive?: boolean } = {}) {
  const writes: ChoiceOp[] = [];
  const actions: string[] = [];
  const notices: string[] = [];
  let revision = 1;
  let tabs: Tabs = { workspace, tabs: [tab("a"), tab("b"), tab("c", options.tabOnly)],
    history: [], selectedChat: "c", previousRecent: null, nextRecent: null, stepTargets: {} };
  const row: SidebarRow = {
    id: workspace, kind: "workspace", depth: 0, parentId: null, expanded: true, hasChildren: false,
    childrenSummary: null, title: "app", detail: "", status: null, counts: [], age: null, help: "app",
    pinned: false, archived: false, missing: false, newChatEnabled: true, dimmed: false, fields: [],
    selection: { workspace, chat: null, agentSession: null, agent: null },
  };
  const tree = (): TreeView => ({
    version: 1, revision: `rev-${revision}`, view: "workspaces", selectedRow: null, archivedStatus: null,
    choicesError: null, prefs: {},
    sections: [{ id: "/r", kind: "project", path: "/r", collapseId: "project:/r", expanded: true,
      title: "r", status: null, fields: [], rows: [row, ...tabs.tabs.filter((t) => !t.tabOnly).map((t) => ({
        ...row, id: `chat:${t.session}`, kind: "chat" as const, selection: { ...row.selection!, chat: t.session },
      }))] }],
  });
  const snapshot: Snapshot = {
    version: 1, sessions: ["a", "b", "c"].map((id) => ({ id, cwd: workspace, workspace, project: "/r", created_at: 1, chair_provider: null, adapter: null })),
    agents: options.live ? { c: [{ id: "orchestrator", role: "o", state: "working", alive: true }] } : {},
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
        remove("c");
      }
    },
  } as unknown as Api;
  const app = new App(api);
  app.workspace = workspace;
  app.openId = "c";
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
    expect(writes.flatMap((op) => op.op === "close_tab" ? [op.chat] : [])).toEqual(["c", "b"]);
    expect(app.openId).toBe("a");
    expect(app.closedTabs).toEqual(["c", "b"]);
    expect(notices).toEqual([]);
  });

  it("waits for a tab-only archive before the second press closes the next tab", async () => {
    const sessions = { open: "c", next: "b", older: "a" };
    const archive = deferred();
    const { app, writes, actions } = harness({ tabOnly: true, beforeArchive: () => archive.promise });
    const closing = [app.closeTab(), app.closeTab()];
    await vi.waitFor(() => expect(actions).toHaveLength(1));
    expect(app.openId).toBe(sessions.open);
    expect(writes.filter((op) => op.op === "close_tab")).toHaveLength(0);
    archive.resolve();
    await Promise.all(closing);
    expect(actions).toEqual(["/api/sessions/archive"]);
    expect(writes.flatMap((op) => op.op === "close_tab" ? [op.chat] : [])).toEqual([sessions.next]);
    expect(app.openId).toBe(sessions.older);
    expect(app.closedTabs).toEqual([sessions.open, sessions.next]);
  });

  it("lets a later close run after a strip read never settles", async () => {
    vi.useFakeTimers();
    try {
      const nextSession = "b";
      const { app, writes } = harness({ beforeTabs: () => new Promise(() => {}) });
      const settled = vi.fn();
      const closing = Promise.all([app.closeTab(), app.closeTab(nextSession)]).then(settled);
      await vi.advanceTimersByTimeAsync(4999);
      expect(writes.filter((op) => op.op === "close_tab")).toHaveLength(1);
      expect(settled).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(1);
      expect(writes.flatMap((op) => op.op === "close_tab" ? [op.chat] : [])).toEqual(["c", nextSession]);
      expect(settled).toHaveBeenCalledOnce();
      await closing;
    } finally {
      vi.useRealTimers();
    }
  });

  it("leaves the nav sequence unchanged when a background tab closes", async () => {
    const { app } = harness();
    const seq = app.navSeq;
    await app.closeTab("a");
    expect(app.navSeq).toBe(seq);
    expect(app.openId).toBe("c");
    expect(app.closedTabs).toEqual(["a"]);
  });

  it("does not add a refused close to the reopen list", async () => {
    const { app } = harness({ refuse: true });
    await app.closeTab();
    expect(app.closedTabs).toEqual([]);
    expect(app.openId).toBe("c");
  });

  it("keeps the reopen list and open chat after cancelling a tab-only close", async () => {
    const { app, actions } = harness({ tabOnly: true, live: true });
    app.closedTabs = ["a"];
    const settled = vi.fn();
    const closing = app.closeTab().then(settled);
    await vi.waitFor(() => expect(app.life.state.confirm).not.toBeNull());
    expect(settled).not.toHaveBeenCalled();
    app.life.close("confirm");
    await closing;
    expect(settled).toHaveBeenCalledOnce();
    expect(app.closedTabs).toEqual(["a"]);
    expect(app.openId).toBe("c");
    const open = vi.spyOn(app.navigation, "openChat");
    app.reopenTab();
    expect(open).toHaveBeenCalledWith("a");
    expect(actions).toEqual([]);
  });

  it("records a confirmed archive only after success and opens the strip selection on refresh", async () => {
    const { app, actions } = harness({ tabOnly: true, live: true });
    const settled = vi.fn();
    const closing = app.closeTab().then(settled);
    await vi.waitFor(() => expect(app.life.state.confirm).not.toBeNull());
    expect(settled).not.toHaveBeenCalled();
    expect(app.closedTabs).toEqual([]);
    expect(await app.life.confirmed()).toBeNull();
    await closing;
    expect(settled).toHaveBeenCalledOnce();
    expect(actions).toEqual(["/api/agent/close", "/api/sessions/archive"]);
    expect(app.closedTabs).toEqual(["c"]);
    expect(app.openId).toBe("b");
  });

  it("does not record a failed archive", async () => {
    const { app } = harness({ tabOnly: true, live: true, failArchive: true });
    const closing = app.closeTab();
    await vi.waitFor(() => expect(app.life.state.confirm).not.toBeNull());
    expect(await app.life.confirmed()).toBe("archive failed");
    expect(app.closedTabs).toEqual([]);
    expect(app.openId).toBe("c");
    expect(app.life.state.confirm).not.toBeNull();
    app.life.close("confirm");
    await closing;
  });

  it("opens the strip selection after a tab-only close without a confirm", async () => {
    const { app } = harness({ tabOnly: true });
    await app.closeTab();
    await vi.waitFor(() => expect(app.openId).toBe("b"));
    expect(app.closedTabs).toEqual(["c"]);
  });
});
