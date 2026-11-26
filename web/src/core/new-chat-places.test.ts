import { describe, expect, it } from "vitest";
import type { TreeView } from "./nav";
import { defaultTarget, targets } from "./new-chat-places";

const workspace = (projectPath: string, title: string, startsChatWorktree = false) => ({
  projectPath, title, startsChatWorktree, newChatEnabled: true, missing: false, archived: false,
  branch: null, diff: null, pr: null,
});
const tree = {
  sections: [],
  projects: [{ path: "/hub", title: "hub", folder: false }, { path: "/notes", title: "notes", folder: true }],
  workspaces: { "/hub/wt/main": workspace("/hub", "main", true), "/hub/wt/fix": workspace("/hub", "fix"), "/notes": workspace("/notes", "notes") },
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
