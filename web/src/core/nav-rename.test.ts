import { describe, expect, it } from "vitest";
import { type Api, type Snapshot } from "./api";
import { App } from "./app";
import { type ChoiceOp, type SidebarRow, type Tabs, type TreeView } from "./nav";

const workspace = "/r/app";
const session = "12345678-abcd-4321-abcd-123456789abc";
const snapshot: Snapshot = {
  version: 1, agents: {},
  sessions: [{ id: session, cwd: workspace, workspace, project: "/r", created_at: 1, chair_provider: null, adapter: null }],
};
const tabs: Tabs = {
  workspace, selectedChat: session, history: [], previousRecent: null, nextRecent: null, stepTargets: {},
  tabs: [{ id: session, session, title: "Fix the hidden chat", status: null, badge: null, canClose: true,
    canHide: true, pending: null, fields: [], children: null }],
};
const row: SidebarRow = {
  id: `chat:${session}`, kind: "chat", depth: 1, parentId: workspace, expanded: false, hasChildren: false,
  childrenSummary: null, title: "Saved sidebar name", detail: "", status: null, counts: [], age: null, help: "",
  pinned: false, archived: false, missing: false, newChatEnabled: false, dimmed: false, fields: [],
  selection: { workspace, chat: session, agentSession: null, agent: null },
};
const tree: TreeView = {
  version: 1, revision: "rev-1", view: "workspaces", selectedRow: null, archivedStatus: null,
  choicesError: null, prefs: {},
  sections: [{ id: "/r", kind: "project", path: "/r", folder: false, collapseId: "project:/r", expanded: true,
    title: "r", status: null, fields: [], rows: [row] }],
};

function harness() {
  const writes: ChoiceOp[] = [];
  const app = new App({
    choice: async (_revision: string, op: ChoiceOp) => {
      writes.push(op);
      return { revision: "rev-2", version: 2 };
    },
  } as unknown as Api);
  app.snapshot = snapshot;
  app.tabs = tabs;
  app.revision = "rev-1";
  return { app, writes };
}

describe("Rename chat prefill", () => {
  it("uses the tab title when a chat has no sidebar row", () => {
    const { app } = harness();
    app.renameChat(session);
    expect(app.view.asking?.value).toBe("Fix the hidden chat");
  });

  it("keeps the sidebar title ahead of the tab title", () => {
    const { app } = harness();
    app.tree = tree;
    app.renameChat(session);
    expect(app.view.asking?.value).toBe("Saved sidebar name");
  });

  it("uses the short id when the chat has neither a row nor a tab", () => {
    const { app } = harness();
    app.tabs = null;
    app.renameChat(session);
    expect(app.view.asking?.value).toBe("12345678");
  });

  it("keeps the rename write and empty-name clear for a chat with no sidebar row", async () => {
    const { app, writes } = harness();
    app.renameChat(session);
    app.view.asking!.save("");
    await app.writes;
    expect(writes).toEqual([{ op: "rename_chat", workspace, chat: session, name: "" }]);
    app.ask(null);
    app.renameChat("unknown");
    expect(app.view.asking).toBeNull();
  });
});
