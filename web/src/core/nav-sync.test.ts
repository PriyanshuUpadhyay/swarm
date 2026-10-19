import { describe, expect, it, vi } from "vitest";
import { ApiError, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { type ChoiceOp, type SidebarRow, type Tab, type Tabs, type TabsView, type TreeView } from "./nav";

const workspace = "/r/app";
const row: SidebarRow = {
  id: workspace, kind: "workspace", depth: 0, parentId: null, expanded: true, hasChildren: false,
  childrenSummary: null, title: "app", detail: "", status: null, counts: [], age: null, help: "app",
  pinned: false, archived: false, missing: false, newChatEnabled: true, dimmed: false, fields: [],
  selection: { workspace, chat: null, agentSession: null, agent: null },
};
const tree: TreeView = {
  version: 1, revision: "rev-1", view: "workspaces", selectedRow: null, archivedStatus: null,
  choicesError: null, prefs: {},
  sections: [{ id: "/r", kind: "project", path: "/r", collapseId: "project:/r", expanded: true,
    title: "r", status: null, fields: [], rows: [row] }],
};
const snapshot: Snapshot = {
  version: 1, agents: {},
  sessions: ["next", "old", "other"].map((id) => ({ id, cwd: workspace, workspace, project: "/r", created_at: 1, chair_provider: null, adapter: null })),
};
const tab = (session: string, pending: Tab["pending"] = null): Tab => ({
  id: session, session, title: session, status: null, badge: null, canClose: true, canHide: true,
  pending, fields: [], children: null, tabOnly: true,
});
const strip = (tabs = [tab("next")], selectedChat: string | null = "next"): Tabs => ({
  workspace, tabs, history: [], selectedChat, previousRecent: null, nextRecent: null, stepTargets: {},
});
const reply = (view: Tabs): TabsView => ({ version: 1, revision: "rev-1", view });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function harness(options: { tree?: TreeView; tabs?: () => Promise<TabsView>; snapshot?: () => Promise<Snapshot>; choice?: () => Promise<{ revision: string; version: number }> } = {}) {
  const writes: ChoiceOp[] = [];
  const api = {
    snapshot: options.snapshot ?? (async () => snapshot),
    tree: async () => options.tree ?? tree,
    tabs: options.tabs ?? (async () => reply(strip())),
    rows: async () => ({ page: { start_offset: 0, end_offset: 0 }, rows: [], usage: null }),
    choice: async (_revision: string, op: ChoiceOp) => {
      writes.push(op);
      return options.choice ? options.choice() : { revision: "rev-2", version: 2 };
    },
  } as unknown as Api;
  const app = new App(api);
  app.workspace = workspace;
  app.restored = true;
  app.revision = "rev-1";
  return { app, writes };
}

describe("sync after a fresh strip read", () => {
  it("opens the selected tab-only chat from a workspace landing without writing choices", async () => {
    const { app, writes } = harness();
    await app.refresh();
    expect(app.view.openId).toBe("next");
    expect(writes).toEqual([]);
  });

  it("restores a tab-only selection through the first tree's workspace fallback", async () => {
    const { app, writes } = harness({ tree: { ...tree, selectedRow: workspace } });
    app.restored = false;
    app.workspace = null;
    await app.refresh();
    expect(app.view.workspace).toBe(workspace);
    expect(app.view.openId).toBe("next");
    expect(writes).toEqual([]);
  });

  it("opens the strip selected chat after the open chat's tab closes elsewhere", async () => {
    const { app, writes } = harness();
    app.openId = "old";
    await app.refresh();
    expect(app.view.openId).toBe("next");
    expect(writes).toEqual([]);
  });

  it.each<Tab["pending"]>([null, "starting", "failed", "closing"])("keeps the open chat when its tab is present with pending %s", async (pending) => {
    const { app } = harness({ tabs: async () => reply(strip([tab("next"), tab("old", pending)])) });
    app.openId = "old";
    await app.refresh();
    expect(app.view.openId).toBe("old");
  });

  it.each([null, "old"])("keeps open chat %s when the strip has no selected chat", async (open) => {
    const { app } = harness({ tabs: async () => reply(strip([], null)) });
    app.openId = open;
    await app.refresh();
    expect(app.view.openId).toBe(open);
  });

  it.each([{ name: "an empty strip", tabs: [] }, { name: "a strip with another chat", tabs: [tab("other")] }])("keeps the workspace landing when selectedChat has no tab in $name", async ({ tabs }) => {
    const { app, writes } = harness({ tabs: async () => reply(strip(tabs, "old")) });
    await app.refresh();
    expect(app.view.openId).toBeNull();
    expect(writes).toEqual([]);
  });

  it("does not restore a chat left after archive while a fresh strip still lists it", async () => {
    const { app, writes } = harness({ tabs: async () => reply(strip([tab("old")], "old")) });
    app.openId = "old";
    app.leaveChat("old");
    await app.refresh();
    expect(app.view.openId).toBeNull();
    await app.refresh();
    expect(app.view.openId).toBeNull();
    expect(writes).toEqual([]);
  });

  it.each([true, false])("an explicit openChat clears the left chat record with write %s", async (write) => {
    const { app } = harness({ tabs: async () => reply(strip([tab("old")], "old")) });
    app.openId = "old";
    app.leaveChat("old");
    app.snapshot = snapshot;
    app.openChat("old", undefined, { write });
    expect(app.view.openId).toBe("old");
    await app.writes;
    app.openChat("other", undefined, { write: false });
    await app.refresh();
    expect(app.view.openId).toBe("old");
  });

  it("does not sync the Archived view", async () => {
    const { app } = harness({ tree: { ...tree, view: "archived" } });
    app.treeName = "archived";
    await app.refresh();
    expect(app.view.openId).toBeNull();
  });

  it("does not use a cached strip when the workspace read fails", async () => {
    const { app } = harness({ tabs: async () => { throw new ApiError(404, "unknown workspace"); } });
    app.tabs = strip();
    await app.refresh();
    expect(app.view.openId).toBeNull();
  });

  it("does not use a strip for a different workspace", async () => {
    const { app } = harness({ tabs: async () => reply({ ...strip(), workspace: "/r/elsewhere" }) });
    await app.refresh();
    expect(app.view.openId).toBeNull();
  });

  it("does not use a cached strip without a current workspace", async () => {
    const { app } = harness();
    app.workspace = null;
    app.tabs = strip();
    await app.refresh();
    expect(app.view.openId).toBeNull();
  });

  it("does not undo navigation while the strip read is in flight", async () => {
    const tabs = deferred<TabsView>();
    const { app, writes } = harness({ tabs: () => tabs.promise });
    app.snapshot = snapshot;
    const refreshing = app.refresh();
    app.openChat("other", undefined, { write: false });
    tabs.resolve(reply(strip()));
    await refreshing;
    expect(app.view.openId).toBe("other");
    expect(writes).toEqual([]);
  });

  it("does not sync a read overtaken by a successful choice write", async () => {
    const tabs = deferred<TabsView>();
    const { app } = harness({ tabs: () => tabs.promise });
    const refreshing = app.refresh();
    await app.choose({ op: "pin", workspace });
    tabs.resolve(reply(strip()));
    await refreshing;
    expect(app.view.openId).toBeNull();
  });

  it("does not sync while a choice write is in flight", async () => {
    const choice = deferred<{ revision: string; version: number }>();
    const { app } = harness({ choice: () => choice.promise });
    const writing = app.choose({ op: "pin", workspace });
    await app.refresh();
    expect(app.view.openId).toBeNull();
    choice.resolve({ revision: "rev-2", version: 2 });
    await writing;
  });

  it("does not undo navigation after the strip arrives while the snapshot is still in flight", async () => {
    const snap = deferred<Snapshot>();
    const view = strip();
    const { app } = harness({ snapshot: () => snap.promise, tabs: async () => reply(view) });
    app.snapshot = snapshot;
    const refreshing = app.refresh();
    await vi.waitFor(() => expect(app.tabs).toBe(view));
    app.openChat("other", undefined, { write: false });
    snap.resolve(snapshot);
    await refreshing;
    expect(app.view.openId).toBe("other");
  });

  it("does not sync after a write lands while the snapshot is still in flight", async () => {
    const snap = deferred<Snapshot>();
    const view = strip();
    const { app } = harness({ snapshot: () => snap.promise, tabs: async () => reply(view) });
    const refreshing = app.refresh();
    await vi.waitFor(() => expect(app.tabs).toBe(view));
    await app.choose({ op: "pin", workspace });
    snap.resolve(snapshot);
    await refreshing;
    expect(app.view.openId).toBeNull();
  });

  it("does not use a read from a workspace left during the request", async () => {
    const tabs = deferred<TabsView>();
    const { app } = harness({ tabs: () => tabs.promise });
    app.tabs = strip();
    const refreshing = app.refresh();
    app.showHome();
    tabs.resolve(reply(strip()));
    await refreshing;
    expect(app.view.workspace).toBeNull();
    expect(app.view.openId).toBeNull();
  });
});
