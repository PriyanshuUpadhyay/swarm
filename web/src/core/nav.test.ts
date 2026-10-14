import { describe, expect, it } from "vitest";
import { ApiError, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { moveTarget, navRows, paletteSections, rankPalette, recentWork, shareTree, type PaletteItem, type Section, type SidebarRow, type TreeView } from "./nav";

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

describe("sidebar lines", () => {
  it("lays out headers and rows with levels and parents", () => {
    expect(navRows(sample).map((r) => `${r.level}:${r.kind}:${r.id}:${r.parentId}`)).toEqual([
      "1:pinned:pinned:null",
      "2:workspace:/r/web:pinned",
      "3:chat:chat:site:/r/web",
      "1:project:/r/app:null",
      "2:workspace:/r/app:/r/app",
      "3:chat:chat:fix:/r/app",
      "4:child:fix/reviewer:chat:fix",
      "3:chat:chat:docs:/r/app",
      "2:workspace:/r/app/wt/feature:/r/app",
    ]);
  });

  it("shows a fold press at once: a folded header hides its rows, a folded row hides deeper rows only", () => {
    const folds = new Map([
      ["pinned", false],
      ["chat:fix", false],
    ]);
    const rows = navRows(sample, folds);
    expect(rows.map((r) => r.id)).toEqual(["pinned", "/r/app", "/r/app", "chat:fix", "chat:docs", "/r/app/wt/feature"]);
    expect(rows.find((r) => r.id === "chat:fix")!.expanded).toBe(false);
    expect(rows[0].expanded).toBe(false);
  });

  it("lists every archived row with no folds", () => {
    const archived = tree([section("/r/app", "project", [appMain], { expanded: false })], "archived");
    expect(navRows(archived).map((r) => [r.id, r.expanded])).toEqual([
      ["/r/app", null],
      ["/r/app", true],
    ]);
  });
});

describe("workspace moves", () => {
  it("moves within a project section, and stops at its ends", () => {
    expect(moveTarget(sample, null, "/r/app", 1)).toBe("/r/app/wt/feature");
    expect(moveTarget(sample, null, "/r/app", -1)).toBeNull();
    expect(moveTarget(sample, null, "/r/app/wt/feature", 1)).toBeNull();
  });

  it("moves a pinned row only past pinned rows of its own project", () => {
    const other = row("/r/blog", "workspace", 0, { pinned: true });
    const site = row("/r/web/wt/b", "workspace", 0, { pinned: true });
    const t = tree([section("pinned", "pinned", [web, other, site])]);
    const s = (id: string, project: string) => ({ id, cwd: project, workspace: id, project, created_at: 1, chair_provider: null, adapter: null });
    const snap: Snapshot = { version: 1, sessions: [s("a", "/r/web"), { ...s("b", "/r/blog"), workspace: "/r/blog" }, { ...s("c", "/r/web"), workspace: "/r/web/wt/b" }], agents: {} };
    snap.sessions[0].workspace = "/r/web";
    expect(moveTarget(t, snap, "/r/web", 1)).toBe("/r/web/wt/b");
  });
});

describe("palette search (jean's fuzzy search)", () => {
  const item = (id: string, group: PaletteItem["group"], recency: number | null = null, subtitle?: string): PaletteItem => ({ id, title: id, group, recency, subtitle, run: () => {} });
  const items = [item("New Chat", "action"), item("Next Workspace", "action", 5), item("app / main", "workspace", 10), item("fix rows", "chat", 20, "app / main"), item("reviewer", "agent", null, "code · fix app")];

  it("finds a title through a typo, and ranks a title match over a subtitle match", () => {
    expect(rankPalette(items, "reviwer").map((i) => i.id)).toEqual(["reviewer"]);
    expect(rankPalette(items, "main").map((i) => i.id)).toEqual(["app / main", "fix rows"]);
  });

  it("lists recent items newest first, then actions, with no query", () => {
    expect(paletteSections(items, " ").map((s) => [s.title, s.items.map((i) => i.id)])).toEqual([
      ["Recent", ["fix rows", "app / main", "Next Workspace"]],
      ["Actions", ["New Chat"]],
    ]);
  });

  it("groups a query's results as Actions, Workspaces, Chats, Agents", () => {
    expect(paletteSections(items, "a").map((s) => s.title)).toEqual(["Actions", "Workspaces", "Chats", "Agents"]);
  });
});

describe("recent work", () => {
  it("orders chats by their newest activity, once each, at most the limit", () => {
    const snap: Snapshot = {
      version: 1,
      sessions: [
        { id: "fix", cwd: "/r/app", workspace: "/r/app", project: "/r/app", created_at: 100, chair_provider: null, adapter: null, last_message_at: 900 },
        { id: "docs", cwd: "/r/app", workspace: "/r/app", project: "/r/app", created_at: 500, chair_provider: null, adapter: null },
        { id: "site", cwd: "/r/web", workspace: "/r/web", project: "/r/web", created_at: 200, chair_provider: null, adapter: null },
      ],
      agents: { site: [{ id: "orchestrator", role: "o", state: "working", alive: true, state_at_s: 700 }] },
    };
    expect(recentWork(sample, snap).map((r) => [r.session, r.workspace])).toEqual([
      ["fix", "/r/app"],
      ["site", "/r/web"],
      ["docs", "/r/app"],
    ]);
    expect(recentWork(sample, snap, 1)).toHaveLength(1);
  });

  it("skips a more row, which the server sends with no selection", () => {
    const more = row("more:/r/app", "more", 1, { selection: null });
    const withMore = tree([section("/r/app", "project", [appMain, docs, more])]);
    expect(recentWork(withMore, null).map((r) => r.session)).toEqual(["docs"]);
  });
});

describe("owner choices", () => {
  /** An App on a fake server that keeps one revision and answers each write in order. */
  function harness(refuse = false) {
    const writes: { revision: string; op: string }[] = [];
    let revision = 1;
    let treeReads = 0;
    const api = {
      snapshot: async (): Promise<Snapshot> => ({ version: revision, sessions: [], agents: {} }),
      tree: async () => {
        treeReads++;
        return { ...sample, revision: `rev-${revision}` };
      },
      tabs: async () => {
        throw new ApiError(404, "unknown workspace");
      },
      choice: async (rev: string, op: { op: string }) => {
        writes.push({ revision: rev, op: op.op });
        await new Promise((r) => setTimeout(r, 1));
        if (refuse || rev !== `rev-${revision}`) throw new ApiError(409, "choices revision is stale");
        revision++;
        return { revision: `rev-${revision}`, version: revision };
      },
    } as unknown as Api;
    const app = new App(api);
    const notices: string[] = [];
    app.notice = (text) => notices.push(text);
    const inner = app as unknown as { refreshSnapshot(): Promise<void> };
    return { app, writes, notices, load: () => inner.refreshSnapshot(), reads: () => treeReads };
  }

  it("writes one at a time, each on the revision the last write returned", async () => {
    const { app, writes, load } = harness();
    await load();
    const a = app.choose({ op: "pin", workspace: "/r/app" });
    const b = app.choose({ op: "collapse", id: "pinned" });
    expect(await Promise.all([a, b])).toEqual([true, true]);
    expect(writes).toEqual([
      { revision: "rev-1", op: "pin" },
      { revision: "rev-2", op: "collapse" },
    ]);
  });

  it("shows a fold at once, and a refused write takes it back, says why, and reads the tree again", async () => {
    const { app, notices, load, reads } = harness(true);
    await load();
    const before = reads();
    const header = app.view.rows.find((r) => r.id === "pinned")!;
    app.toggle(header.id);
    expect(app.view.rows.find((r) => r.id === "pinned")!.expanded).toBe(false);
    expect(app.view.rows.map((r) => r.id)).not.toContain("/r/web");
    await new Promise((r) => setTimeout(r, 10));
    expect(notices).toEqual(["Your choices changed elsewhere. The list was reloaded; try again."]);
    expect(app.view.rows.find((r) => r.id === "pinned")!.expanded).toBe(true);
    expect(reads()).toBeGreaterThan(before);
  });
});

describe("a new chat", () => {
  it("opens once the tree lists it, since the server's tree can lag the start", async () => {
    let reads = 0;
    const writes: { op: string; chat?: string }[] = [];
    const withNew = tree([section("/r/app", "project", [appMain, chat("new", "/r/app")])]);
    const api = {
      snapshot: async (): Promise<Snapshot> => ({ version: reads, sessions: [], agents: {} }),
      tree: async () => (++reads > 2 ? withNew : sample),
      tabs: async () => {
        throw new ApiError(404, "unknown workspace");
      },
      post: async () => ({ session: "new" }),
      choice: async (rev: string, op: { op: string; chat?: string }) => {
        writes.push({ op: op.op, chat: op.chat });
        if (!withNew.sections[0].rows.some((r) => r.selection?.chat === op.chat)) throw new ApiError(400, "unknown chat in workspace");
        return { revision: rev, version: 1 };
      },
    } as unknown as Api;
    const app = new App(api);
    app.notice = () => {};
    await (app as unknown as { refreshSnapshot(): Promise<void> }).refreshSnapshot();
    await app.startChat("/r/app");
    expect(reads).toBeGreaterThan(2);
    expect(app.view.openId).toBe("new");
    expect(writes.filter((w) => w.chat === "new").map((w) => w.op)).toContain("open_tab");
  });
});

describe("a chat started in a tab", () => {
  it("opens as a tab only, and closing its tab archives it to Recently closed", async () => {
    const writes: Record<string, unknown>[] = [];
    const actions: { path: string; body: unknown }[] = [];
    const withNew = tree([section("/r/app", "project", [appMain, chat("new", "/r/app")])]);
    const tab = { id: "new", session: "new", title: "new", status: null, badge: null, canClose: true, canHide: true, pending: null, fields: [], children: null, tabOnly: true };
    const api = {
      snapshot: async (): Promise<Snapshot> => ({ version: 1, sessions: [], agents: {} }),
      tree: async () => withNew,
      tabs: async (workspace: string) => ({ version: 1, revision: "rev-1", view: { workspace, tabs: [tab], history: [], selectedChat: "new", previousRecent: null, nextRecent: null, stepTargets: {} } }),
      post: async () => ({ session: "new" }),
      action: async (path: string, body: unknown) => void actions.push({ path, body }),
      choice: async (rev: string, op: Record<string, unknown>) => {
        writes.push(op);
        return { revision: rev, version: 1 };
      },
    } as unknown as Api;
    const app = new App(api);
    app.notice = () => {};
    await (app as unknown as { refreshSnapshot(): Promise<void> }).refreshSnapshot();
    await app.startChat("/r/app", {}, "", true);
    expect(writes.find((w) => w.op === "open_tab")).toMatchObject({ chat: "new", tab_only: true });
    await (app as unknown as { refreshSnapshot(): Promise<void> }).refreshSnapshot();
    await app.closeTab("new");
    await new Promise((r) => setTimeout(r, 0));
    expect(actions).toEqual([{ path: "/api/sessions/archive", body: { ids: ["new"] } }]);
    expect(writes.some((w) => w.op === "close_tab")).toBe(false);
  });
});

describe("tree refresh", () => {
  it("keeps the old object of each row and section that did not change", () => {
    const next = JSON.parse(JSON.stringify(sample)) as TreeView;
    next.sections[1].rows[3] = { ...next.sections[1].rows[3], age: "2m" };
    const shared = shareTree(sample, next);
    expect(shared.sections[0]).toBe(sample.sections[0]);
    expect(shared.sections[1]).not.toBe(sample.sections[1]);
    expect(shared.sections[1].rows[0]).toBe(sample.sections[1].rows[0]);
    expect(shared.sections[1].rows[3]).not.toBe(sample.sections[1].rows[3]);
    expect(shared.sections[1].rows[3].age).toBe("2m");
  });
});
