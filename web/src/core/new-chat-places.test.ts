import { describe, expect, it } from "vitest";
import type { TreeView } from "./nav";
import { targets } from "./new-chat-places";

const row = (id: string, startsChatWorktree: boolean) => ({ id, kind: "workspace", newChatEnabled: true, startsChatWorktree });
const tree = {
  sections: [
    { kind: "project", path: "/hub", folder: false, title: "hub", rows: [row("/hub/wt/main", true), row("/hub/wt/fix", false)] },
    { kind: "project", path: "/notes", folder: true, title: "notes", rows: [row("/notes", false)] },
  ],
} as unknown as TreeView;

describe("new chat places", () => {
  it("starts on a hub's main for a new worktree, and in a plain folder itself", () => {
    expect(targets(tree, "/hub/wt/fix")).toEqual([
      { cwd: "/hub/wt/main", project: "/hub", title: "hub", git: true },
      { cwd: "/notes", project: "/notes", title: "notes", git: false },
    ]);
  });
});
