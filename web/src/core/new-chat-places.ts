// The composer's projects and places (ADR 0149): which folder a new chat starts in, and the
// words and checks for each place.

import { workspaceRow, type TreeView } from "./nav";

/** Where a new chat runs; `new` makes the chat its own worktree. */
export type Place = { kind: "new" } | { kind: "here" } | { kind: "branch"; branch: string } | { kind: "pr"; number: number } | { kind: "issue"; number: number };

/** A project the composer can start in: the folder a new chat names, and whether it is a git
 *  project, which can make a new worktree; a plain folder starts the chat in itself. */
export type Target = { cwd: string; project: string; title: string; git: boolean };

/** What a new worktree gets, for the composer's card. */
export type Preview = { base: string | null; fetchedUnixS: number | null; setup: string | null };

/** Whether the view shows the composer in its page: Home, and a workspace's landing that takes
 *  new chats. Elsewhere ⌘N opens it over the view. */
export const composerInPage = (view: { openId: string | null; workspace: string | null; tree: TreeView | null }) =>
  !view.openId && (!view.workspace || workspaceRow(view.tree, view.workspace)?.newChatEnabled === true);

/** The projects the composer offers, each with the folder a new chat in it names: a hub's main
 *  checkout, else the open workspace, else the project's own folder. A git project can make a new worktree; a plain
 *  folder starts the chat in itself. */
export function targets(tree: TreeView | null, workspace: string | null): Target[] {
  if (!tree) return [];
  const folders = Object.entries(tree.workspaces);
  return tree.projects.flatMap((project) => {
    const eligible = folders.filter(([, w]) => w.projectPath === project.path && w.newChatEnabled);
    const main = eligible.find(([, w]) => w.startsChatWorktree)?.[0];
    const current = eligible.find(([path]) => path === workspace)?.[0];
    const own = workspaceRow(tree, project.path);
    const fallback = own?.newChatEnabled || (!own && !project.folder) ? project.path : null;
    const cwd = main ?? current ?? fallback;
    return cwd ? [{ cwd, project: project.path, title: project.title, git: !project.folder }] : [];
  });
}

/** Whether the project at `path` has a folder a new chat can start in. */
export const canStartIn = (tree: TreeView | null, path: string | null) => targets(tree, null).some((t) => t.project === path);

/** The project the composer opens on: the one of `workspace`, else the first. */
export function defaultTarget(list: Target[], tree: TreeView | null, workspace: string | null): Target | null {
  const project = workspaceRow(tree, workspace)?.projectPath ?? workspace;
  return list.find((t) => t.project === project) ?? list[0] ?? null;
}

/** The place a target starts on unless the owner picks another. */
export const defaultPlace = (target: Target): Place => ({ kind: target.git ? "new" : "here" });

/** The chip's words for a place. */
export function placeLabel(place: Place, target: Target): string {
  if (!target.git) return "In folder";

  switch (place.kind) {
    case "new":
      return "New worktree";
    case "here":
      return "In this folder";

    case "branch":
      return place.branch ? `Branch ${place.branch}` : "Pick a branch";
    case "pr":
      return place.number ? `PR #${place.number}` : "Pick a pull request";
    case "issue":
      return place.number ? `Issue #${place.number}` : "Pick an issue";

    default: {
      const _exhaustive: never = place;
      return _exhaustive;
    }
  }
}

/** Whether a send can go with this place: a branch, PR or issue needs its pick. */
export function placeReady(place: Place): boolean {
  switch (place.kind) {
    case "new":
    case "here":
      return true;

    case "branch":
      return place.branch !== "";
    case "pr":
    case "issue":
      return place.number > 0;
    default: {
      const _exhaustive: never = place;
      return _exhaustive;
    }
  }
}
