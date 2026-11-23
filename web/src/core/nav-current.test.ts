import { describe, expect, it } from "vitest";
import { ApiError, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { currentRow, navRows, type TreeView } from "./nav";
import { row, section, tree, appMain, docs, web, webChat, sample } from "./nav-fixtures";

describe("current sidebar row", () => {
  it("uses the visible chat, then its workspace for a chat with no sidebar row or no open chat", () => {
    const rows = navRows(sample);
    expect(currentRow(rows, "docs", "/r/app")).toBe("chat:docs");
    expect(currentRow(rows, "hidden", "/r/app")).toBe("/r/app");
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
    expect(currentRow(rows, "hidden", "/r/web")).toBe("pinned");
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
      snapshot: async (): Promise<Snapshot> => ({ version: 1, sessions: ["docs", "hidden"].map((id) => ({ id, cwd: "/r/app", workspace: "/r/app", project: "/r/app", created_at: 1, chair_provider: null, adapter: null })), agents: {} }),
      tree: async () => current,
      tabs: async () => { throw new ApiError(404, "unknown workspace"); },
      rows: async () => ({ page: { start_offset: 0, end_offset: 0 }, rows: [], usage: null }),
      choice: async () => ({ revision: "rev-1", version: 1 }),
    } as unknown as Api;
    const app = new App(api);
    const refresh = () => app.refresh();
    return { app, refresh, setTree: (next: TreeView) => { current = next; } };
  }

  it("follows a chat with no sidebar row to its workspace on open and refresh", async () => {
    const { app, refresh } = harness();
    await refresh();
    app.openChat("hidden");
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

  it("keeps the archived keyboard row when an opened live chat has no current row", async () => {
    const archived = row("archived-chat:older", "archived-chat", 1);
    const archivedTree = tree([section("archived", "project", [archived])], "archived");
    const { app, refresh } = harness(archivedTree);
    app.treeName = "archived";
    await refresh();
    app.selected = archived.id;
    app.openChat("docs");
    expect(app.view.selected).toBe(archived.id);
    await refresh();
    expect(app.view.selected).toBe(archived.id);
  });

  it.each(["header", "workspace"])("follows the open chat after its %s expands", async (container) => {
    const folded = tree([
      section("pinned", "pinned", [web, webChat]),
      section("/r", "project", [{ ...appMain, expanded: container !== "workspace" }, docs], { expanded: container !== "header" }),
    ]);
    const { app, refresh, setTree } = harness(folded);
    await refresh();
    app.workspace = "/r/web";
    app.openId = "site";
    app.openChat("docs");
    expect(app.view.selected).toBe(container === "header" ? "/r" : "/r/app");
    await app.writes;
    setTree(tree([section("pinned", "pinned", [web, webChat]), section("/r", "project", [appMain, docs])]));
    await refresh();
    expect(app.view.rows.some((r) => r.id === "chat:docs")).toBe(true);
    expect(app.view.selected).toBe("chat:docs");
  });

  it("keeps the row opened by a manual expand instead of following the chat", async () => {
    const folded = tree([section("/r", "project", [{ ...appMain, expanded: false }, docs])]);
    const { app, refresh, setTree } = harness(folded);
    await refresh();
    app.workspace = "/r/app";
    app.openChat("docs");
    await app.writes;
    await refresh();
    expect(app.view.selected).toBe("/r/app");
    app.sidebar.expand();
    expect(app.view.selected).toBe("/r/app");
    await app.writes;
    setTree(tree([section("/r", "project", [appMain, docs])]));
    await refresh();
    expect(app.view.rows.some((r) => r.id === "chat:docs")).toBe(true);
    expect(app.view.selected).toBe("/r/app");
  });

  it.each(["restore", "keyboard move"])("keeps the keyboard row after expansion for a %s", async (reason) => {
    const folded = tree([section("pinned", "pinned", [web, webChat]), section("/r", "project", [appMain, docs], { expanded: false })]);
    const { app, refresh, setTree } = harness(folded);
    await refresh();
    app.workspace = "/r/web";
    app.selected = "/r";
    app.openChat("docs", undefined, { write: reason !== "restore" });
    if (reason === "keyboard move") app.selected = "pinned";
    const selected = app.selected;
    await app.writes;
    setTree(tree([section("pinned", "pinned", [web, webChat]), section("/r", "project", [appMain, docs])]));
    await refresh();
    expect(app.view.selected).toBe(selected);
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

