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
});
