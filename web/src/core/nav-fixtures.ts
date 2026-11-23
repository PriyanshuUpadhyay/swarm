import type { Section, SidebarRow, TreeView } from "./nav";

const sel = (workspace: string | null, chat: string | null = null) => ({ workspace, chat, agentSession: null, agent: null });

export function row(id: string, kind: SidebarRow["kind"], depth: number, extra: Partial<SidebarRow> = {}): SidebarRow {
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
    newChatEnabled: true, startsChatWorktree: false,
    dimmed: false,
    fields: [],
    selection: sel(kind === "workspace" ? id : null),
    ...extra,
  };
}

export const chat = (session: string, workspace: string, extra: Partial<SidebarRow> = {}) =>
  row(`chat:${session}`, "chat", 1, { parentId: workspace, selection: sel(workspace, session), ...extra });

export function section(id: string, kind: Section["kind"], rows: SidebarRow[], extra: Partial<Section> = {}): Section {
  return { id, kind, path: kind === "project" ? id : null, folder: false, collapseId: kind === "project" ? `project:${id}` : "pinned", expanded: true, title: id.split("/").pop()!, status: null, fields: [], rows, ...extra };
}

export const tree = (sections: Section[], view: TreeView["view"] = "workspaces"): TreeView => ({
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
export const appMain = row("/r/app", "workspace", 0, { hasChildren: true });
export const appFeature = row("/r/app/wt/feature", "workspace", 0);
export const fixRows = chat("fix", "/r/app", { hasChildren: true, expanded: true });
export const fixChild = row("fix/reviewer", "child", 2, { parentId: "chat:fix" });
export const docs = chat("docs", "/r/app");
export const web = row("/r/web", "workspace", 0, { pinned: true, hasChildren: true });
export const webChat = chat("site", "/r/web");
export const sample = tree([section("pinned", "pinned", [web, webChat]), section("/r/app", "project", [appMain, fixRows, fixChild, docs, appFeature])]);
