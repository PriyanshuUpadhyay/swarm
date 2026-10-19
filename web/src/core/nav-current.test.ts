import { describe, expect, it } from "vitest";
import { ApiError, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { currentRow, navRows, type Section, type SidebarRow, type TreeView } from "./nav";

const sel = (workspace: string | null, chat: string | null = null) => ({ workspace, chat, agentSession: null, agent: null });

function row(id: string, kind: SidebarRow["kind"], depth: number, extra: Partial<SidebarRow> = {}): SidebarRow {
  return {
    id,
    kind,
    depth,
    parentId: null,
    expanded: true,
    hasChildren: false,
    childrenSummary: null,
    title: id,
    detail: "",
    status: null,
    counts: [],
    age: null,
    help: id,
    pinned: false,
    archived: false,
    missing: false,
    newChatEnabled: true,
    dimmed: false,
    fields: [],
    selection: sel(kind === "workspace" ? id : null),
    ...extra,
  };
}

const chat = (session: string, workspace: string, extra: Partial<SidebarRow> = {}) =>
  row(`chat:${session}`, "chat", 1, { parentId: workspace, selection: sel(workspace, session), ...extra });

function section(id: string, kind: Section["kind"], rows: SidebarRow[], extra: Partial<Section> = {}): Section {
  return { id, kind, path: kind === "project" ? id : null, collapseId: kind === "project" ? `project:${id}` : "pinned", expanded: true, title: id.split("/").pop()!, status: null, fields: [], rows, ...extra };
}

const tree = (sections: Section[], view: TreeView["view"] = "workspaces"): TreeView => ({
  version: 1,
  revision: "rev-1",
  view,
  sections,
  selectedRow: null,
  archivedStatus: null,
  choicesError: null,
  prefs: {},
});

// Project /r/app with workspaces main (two chats, the first with a child) and feature; a pinned /r/web.
const appMain = row("/r/app", "workspace", 0, { hasChildren: true });
const appFeature = row("/r/app/wt/feature", "workspace", 0);
const fixRows = chat("fix", "/r/app", { hasChildren: true, expanded: true });
const fixChild = row("fix/reviewer", "child", 2, { parentId: "chat:fix" });
const docs = chat("docs", "/r/app");
const web = row("/r/web", "workspace", 0, { pinned: true, hasChildren: true });
const webChat = chat("site", "/r/web");
const sample = tree([section("pinned", "pinned", [web, webChat]), section("/r/app", "project", [appMain, fixRows, fixChild, docs, appFeature])]);

describe("current sidebar row", () => {
  it("uses the visible chat, then its workspace for a tab-only chat or no open chat", () => {
    const rows = navRows(sample);
    expect(currentRow(rows, "docs", "/r/app")).toBe("chat:docs");
    expect(currentRow(rows, "tab-only", "/r/app")).toBe("/r/app");
    expect(currentRow(rows, null, "/r/app")).toBe("/r/app");
  });

  it("uses the visible workspace when its chat is folded away", () => {
    const rows = navRows(sample, new Map([["/r/app", false]]));
    expect(rows.some((r) => r.id === "chat:docs")).toBe(false);
    expect(currentRow(rows, "docs", "/r/app")).toBe("/r/app");
  });

  it("uses the first section header that contains the hidden workspace", () => {
    const rows = navRows(sample, new Map([["pinned", false], ["project:/r/app", false]]));
    expect(currentRow(rows, "docs", "/r/app")).toBe("/r/app");
    expect(currentRow(rows, "tab-only", "/r/web")).toBe("pinned");
    const duplicate = tree([section("pinned", "pinned", [web]), section("/r/web", "project", [web])]);
    expect(currentRow(navRows(duplicate, new Map([["pinned", false], ["project:/r/web", false]])), "site", "/r/web")).toBe("pinned");
  });

  it("returns null without a matching chat, workspace or section", () => {
    expect(currentRow(navRows(sample), "unknown", "/missing")).toBeNull();
    expect(currentRow(navRows(sample), null, null)).toBeNull();
    expect(currentRow([], "docs", "/r/app")).toBeNull();
  });
});

describe("the sidebar keyboard row", () => {
  function harness(initial = sample) {
    let current = initial;
    const api = {
      snapshot: async (): Promise<Snapshot> => ({ version: 1, sessions: ["docs", "tab-only"].map((id) => ({ id, cwd: "/r/app", workspace: "/r/app", project: "/r/app", created_at: 1, chair_provider: null, adapter: null })), agents: {} }),
      tree: async () => current,
      tabs: async () => { throw new ApiError(404, "unknown workspace"); },
      rows: async () => ({ page: { start_offset: 0, end_offset: 0 }, rows: [], usage: null }),
      choice: async () => ({ revision: "rev-1", version: 1 }),
    } as unknown as Api;
    const app = new App(api);
    const refresh = () => (app as unknown as { refreshSnapshot(): Promise<void> }).refreshSnapshot();
    return { app, refresh, setTree: (next: TreeView) => { current = next; } };
  }

  it("follows a tab-only chat to its workspace on open and refresh", async () => {
    const { app, refresh } = harness();
    await refresh();
    app.openChat("tab-only");
    expect(app.view.selected).toBe("/r/app");
    await refresh();
    expect(app.view.selected).toBe("/r/app");
  });

  it("uses the visible workspace instead of a chat under its fold", async () => {
    const folded = tree([section("/r/app", "project", [{ ...appMain, expanded: false }, docs])]);
    const { app, refresh } = harness(folded);
    await refresh();
    app.openChat("docs");
    expect(app.view.rows.some((r) => r.id === "chat:docs")).toBe(false);
    expect(app.view.selected).toBe("/r/app");
  });

  it("moves a keyboard row that left the list to the open chat's workspace", async () => {
    const { app, refresh, setTree } = harness();
    await refresh();
    app.openChat("docs");
    expect(app.view.selected).toBe("chat:docs");
    setTree(tree([section("pinned", "pinned", [web]), section("/r/app", "project", [appMain])]));
    await refresh();
    expect(app.view.selected).toBe("/r/app");
    expect(app.view.selected).not.toBe(app.view.rows[0].id);
  });

  it("keeps a visible keyboard row on restore and refresh, and uses the first row without a current row", async () => {
    const { app, refresh, setTree } = harness();
    await refresh();
    app.openChat("docs", undefined, { write: false });
    expect(app.view.selected).toBe("pinned");
    await refresh();
    expect(app.view.selected).toBe("pinned");
    app.openChat("docs");
    setTree(tree([section("pinned", "pinned", [web])]));
    await refresh();
    expect(app.view.selected).toBe("pinned");
  });
});

