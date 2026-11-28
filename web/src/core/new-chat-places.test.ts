import { describe, expect, it } from "vitest";
import type { TreeView } from "./nav";
import { folderFacts } from "./nav-fixtures";
import { defaultTarget, targets } from "./new-chat-places";

const tree = {
  sections: [],
  projects: [{ path: "/hub", title: "hub", folder: false }, { path: "/notes", title: "notes", folder: true }],
  workspaces: {
    "/hub/wt/main": folderFacts("/hub", "main", { startsChatWorktree: true }),
    "/hub/wt/fix": folderFacts("/hub", "fix"),
    "/notes": folderFacts("/notes", "notes"),
  },
} as unknown as TreeView;

describe("new chat places", () => {
  it("starts on a hub's main for a new worktree, and in a plain folder itself", () => {
    const list = targets(tree, "/hub/wt/fix");
    expect(list).toEqual([
      { cwd: "/hub/wt/main", project: "/hub", title: "hub", git: true },
      { cwd: "/notes", project: "/notes", title: "notes", git: false },
    ]);
    expect(defaultTarget(list, tree, "/notes")?.cwd).toBe("/notes");
  });
  it("offers only worktree places for an archived git root, not an archived plain folder", () => {
    const archived = (projectPath: string) => folderFacts(projectPath, projectPath, { archived: true, newChatEnabled: false });
    const catalog = { ...tree, workspaces: { "/hub": archived("/hub"), "/notes": archived("/notes") } };
    expect(targets(catalog, null)).toEqual([{ cwd: "/hub", project: "/hub", title: "hub", git: true, worktreeOnly: true }]);
  });
});
