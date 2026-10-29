import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";
import type { Api, Snapshot } from "./api";
import { chatRows, navRows, paletteSources, recentWork, type SidebarRow, type TreeView } from "./nav";

const workspace = "/project/work";
const ws: SidebarRow = {
  id: workspace, kind: "workspace", depth: 0, parentId: null, expanded: false, hasChildren: true,
  childrenSummary: null, title: "Work", detail: "", status: null, counts: [], age: null, help: "",
  pinned: false, archived: false, missing: false, newChatEnabled: true, dimmed: false, fields: [],
  selection: { workspace, chat: null, agentSession: null, agent: null },
};
const chat = (id: string): SidebarRow => ({ ...ws, id: `chat:${id}`, kind: "chat", title: id,
  parentId: workspace, depth: 1, selection: { ...ws.selection!, chat: id } });
const tree: TreeView = { version: 1, revision: "r1", view: "workspaces", selectedRow: null,
  archivedStatus: null, choicesError: null, prefs: {}, sections: [{ id: "project", kind: "project",
    path: "/project", folder: false, collapseId: "project:/project", expanded: true,
    title: "Project", status: null, fields: [], rows: [ws] }] };
const snapshot: Snapshot = { version: 1, agents: {}, sessions: ["one", "two"].map((id, i) => ({
  id, cwd: workspace, workspace, project: "/project", created_at: i + 1, chair_provider: null, adapter: null,
})) };
function harness() {
  const api = { snapshot: async () => snapshot, tree: async (name: string) => name === "archived" ? { ...tree, view: "archived", sections: [] } : tree,
    rows: async () => ({ page: { start_offset: 0, end_offset: 0 }, rows: [], usage: null }),
    choice: async () => ({ version: 2, revision: "r2" }),
  } as unknown as Api;
  const app = new App(api);
  app.tree = tree;
  app.snapshot = snapshot;
  app.rows = navRows(tree);
  app.revision = "r1";
  return app;
}
afterEach(() => vi.unstubAllGlobals());

describe("navigation UX", () => {
  it("finds catalog chats beyond folded and capped rows", () => {
    const catalog = { ...tree, chats: [chat("one"), chat("two")] };
    expect(chatRows(catalog).map((r) => r.title)).toEqual(["one", "two"]);
    expect(paletteSources(catalog, snapshot)[0].chats).toHaveLength(2);
    expect(recentWork(catalog, snapshot).map((r) => r.session)).toEqual(["two", "one"]);
    expect(navRows(catalog).some((r) => r.kind === "chat")).toBe(false);
  });
  it("keeps live navigation data when the sidebar shows archives", async () => {
    const app = harness();
    app.restored = true;
    app.sidebar.setTreeName("archived");
    await app.refresh();
    expect(app.tree?.view).toBe("workspaces");
    expect(app.rows).toEqual([]);
    expect(paletteSources(app.tree, app.snapshot)).toHaveLength(1);
  });
  it("focuses restored chats without writing choices", () => {
    const app = harness();
    app.navigation.focusOpened = vi.fn();
    app.openChat("one", undefined, { write: false });
    expect(app.navigation.focusOpened).toHaveBeenCalledWith("one", false, expect.any(Function));
  });
  it("reports a failed chat link before restoring the saved selection", () => {
    const app = harness();
    Object.defineProperty(app, "urlChat", { value: "missing" });
    app.notice = vi.fn();
    app.navigation.afterNav(null);
    expect(app.notice).toHaveBeenCalledWith("This chat link is unavailable. The chat may be archived or removed.", true);
  });
  it("restores the Recent keyboard row to the open chat", () => {
    const app = harness();
    app.sidebarView = 2;
    app.openChat("one", undefined, { write: false });
    expect(app.recentSelected).toBe("one");
  });
  it("focuses the Recent row without moving focus back to its list", () => {
    const app = harness();
    app.openId = "one";
    const row = { focus: vi.fn() };
    const list = { focus: vi.fn() };
    vi.stubGlobal("document", { querySelector: (selector: string) => selector.includes("[tabindex") ? row : list });
    vi.stubGlobal("requestAnimationFrame", vi.fn());
    app.sidebar.setSidebarView(2);
    expect(row.focus).toHaveBeenCalledOnce();
    expect(list.focus).not.toHaveBeenCalled();
  });
  it("waits for the new list commit and consumes its focus request once", () => {
    const app = harness();
    const querySelector = vi.fn<() => { focus: () => void } | null>(() => null);
    const row = { focus: vi.fn() };
    vi.stubGlobal("document", { querySelector });
    app.sidebar.setSidebarView(2);
    querySelector.mockReturnValue(row);
    app.sidebar.focusRequestedView();
    app.sidebar.focusRequestedView();
    expect(row.focus).toHaveBeenCalledOnce();
  });
  it("does not fold hidden tree rows in Recent", () => {
    const app = harness();
    app.sidebarView = 2;
    app.selected = workspace;
    app.sidebar.toggle = vi.fn();
    app.sidebar.collapse();
    app.sidebar.expand();
    expect(app.sidebar.toggle).not.toHaveBeenCalled();
  });
  it.each([null, "archived", "read-only"])("explains why New Chat cannot run in %s", async (reason) => {
    const app = harness();
    if (reason) {
      app.workspace = workspace;
      app.tree = { ...tree, sections: [{ ...tree.sections[0], rows: [{ ...ws, newChatEnabled: false, archived: reason === "archived" }] }] };
    }
    app.notice = vi.fn();
    await app.startChat();
    expect(app.notice).toHaveBeenCalledWith(reason ? `This workspace is ${reason}.` : "Pick a workspace to start a chat.");
  });
  it("disables palette actions whose command cannot run and checks again on execution", () => {
    const app = harness();
    app.setPalette(true);
    const find = app.paletteItems().find((item) => item.id === "action:find.open")!;
    expect(find.disabledReason).toBeTruthy();
    app.reading.openFind = vi.fn();
    find.run();
    expect(app.reading.openFind).not.toHaveBeenCalled();
  });
  it("lists shortcut commands in the palette", () => {
    const app = harness();
    const items = app.paletteItems();
    expect(items.find((item) => item.id === "action:recent.closed")?.shortcut).toBe("Shift+Meta+T");
    expect(app.registry.list().find((item) => item.id === "chat.9")?.chord).toBe("Meta+9");
  });
  it("does not run tree commands from the page body or a row button", () => {
    const app = harness();
    const button = { closest: (selector: string) => selector.includes("button") ? {} : {} };
    const body = { closest: () => null };
    const doc: { body: object; activeElement: { closest: (selector: string) => object | null } } = { body, activeElement: body };
    vi.stubGlobal("document", doc);
    expect(app.actions.commands().find((c) => c.id === "tree.open")!.when!()).toBe(false);
    doc.activeElement = button;
    expect(app.actions.commands().find((c) => c.id === "tree.open")!.when!()).toBe(false);
  });
});
