// Archive, Remove worktree and Prune for one workspace row (ADR 0055, ADR 0142).

import type { App } from "./app";
import { message } from "./app-types";
import { plural, type Confirm } from "./lifecycle-state";

/** What the workspace actions need from Lifecycle. */
export type WorkspaceActions = {
  api: App["api"];
  app: App;
  ask: (confirm: Confirm) => Promise<void>;
  runAction: (run: () => Promise<unknown>, leaveWorkspace: string | null) => Promise<void>;
};

type Impact = {
  liveAgents: number;
  confirmationRequired: boolean;
  dirtyFiles: string[];
  ignoredCount: number;
  firstIgnoredPath: string | null;
  canDelete: boolean;
  deleteBlocker: string | null;
  prunePaths: string[];
};

async function impact(a: WorkspaceActions, workspace: string): Promise<Impact | null> {
  try {
    return await a.api.get<Impact>("/api/workspace/impact", { workspace });
  } catch (err) {
    a.app.notice(message(err), true);
    return null;
  }
}

/** Archive asks only while agents run, as Swift does. It keeps the folder and frees no disk, and
 *  says so; Remove worktree frees the disk (ADR 0055). */
export async function archiveWorkspace(a: WorkspaceActions, workspace: string) {
  const found = await impact(a, workspace);
  if (!found) return;
  const kept = "The folder stays on disk. Remove worktree frees its space.";
  const run = async () => {
    await a.api.post("/api/workspace/archive", { workspace });
    a.app.notice(`Archived. ${kept}`);
  };
  if (!found.confirmationRequired) return void a.runAction(run, workspace);
  void a.ask({
    title: "Archive workspace?",
    body: `${plural(found.liveAgents, "agent")} still running. Archiving stops them. ${kept}`,
    action: "Archive Workspace",
    run: async () => {
      await run();
      a.app.leaveWorkspace(workspace);
    },
  });
}

export async function deleteWorkspace(a: WorkspaceActions, workspace: string) {
  const found = await impact(a, workspace);
  if (!found) return;
  if (!found.canDelete) return a.app.notice(found.deleteBlocker ?? "This worktree cannot be removed.", true);
  const lines = [`Remove the worktree folder at ${workspace}? The branch stays, so New Workspace can reopen it. Git refuses uncommitted changes or unpushed commits.`];
  if (found.confirmationRequired) lines.push(`${plural(found.liveAgents, "agent")} still running. Removing stops them before the folder goes.`);
  if (found.ignoredCount) lines.push(`${plural(found.ignoredCount, "ignored file")} will be deleted too, such as ${found.firstIgnoredPath}.`);
  if (found.deleteBlocker) lines.push(found.deleteBlocker);
  void a.ask({
    title: "Remove worktree?",
    body: lines.join("\n\n"),
    action: "Remove Worktree",
    blocked: found.deleteBlocker !== null,
    cancelFirst: true,
    run: async () => {
      await a.api.post("/api/workspace/delete", { workspace });
      a.app.leaveWorkspace(workspace);
    },
  });
}

export async function pruneWorkspace(a: WorkspaceActions, workspace: string, project: string) {
  const found = await impact(a, workspace);
  if (!found) return;
  void a.ask({
    title: "Prune missing worktrees?",
    body: `Prune removes the worktree records for these missing folders in ${project}:\n\n${found.prunePaths.join("\n")}`,
    action: "Prune",
    run: () => a.api.post("/api/workspace/prune", { workspace }),
  });
}
