import type { SwitchCarry } from "./composer";

export type Mode = "new-branch" | "existing-branch" | "pr" | "issue";

/** `GET /api/workspace/new-options`. */
export type NewOptions = {
  defaultBranch: string | null;
  branches: string[];
  bases: string[];
  folder: string;
  prefix: string;
  setup_script: string | null;
};

export type Issue = { number: number; title: string; body: string; url?: string; labels?: { name: string }[] };

export type NewRequest = {
  project: string;
  name: string;
  mode: Mode;
  base?: string;
  branch?: string;
  pr?: number;
  issue?: number;
  folder?: string;
  prefix?: string;
};

/** One setup script run, from its `setup` stream frames and the create reply. */
export type SetupRun = {
  workspace: string | null;
  state: "running" | "completed" | "failed" | "timeout";
  output: string;
  exitCode: number | null;
  error: string | null;
};

export type Create = { state: "working" | "failed" | "kept"; error: string | null; workspace: string | null; setup: SetupRun | null };

export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** The End or Archive dialog's words for `chats` chats (`title` names the first) with `agents` open
 *  panes, `busy` of them in a turn. */
export function endConfirm(chats: number, title: string, archive: boolean, agents: number, busy: number): Omit<Confirm, "run"> {
  const many = chats > 1;
  const verb = archive ? "Archiving" : "Ending";
  return {
    title: `${archive ? "Archive" : "End"} ${many ? `${chats} chats` : `“${title}”`}?`,
    body: `${busy ? `${plural(busy, "agent")} still working. ${verb} stops them.` : `${verb} closes ${many ? "their" : "its"} agent${agents > 1 ? "s" : ""}.`}${archive ? (many ? " The chats move to Archived." : " The chat moves to Archived.") : ""}`,
    action: `${archive ? "Archive" : "End"} ${many ? `${chats} Chats` : "Chat"}`,
  };
}

/** A switch phase after which nothing more runs. */
export function isSettled(phase: string): boolean {
  return phase === "completed" || phase === "failed" || phase === "cancelled";
}

/** A switch model run, from its `handoff` stream frames (P3b). */
export type Handoff = { old: string; phase: string; canCancel: boolean; error: string | null; carry?: SwitchCarry };

export type Confirm = { title: string; body: string; action: string; blocked?: boolean; run: () => Promise<unknown> };

/** One row of `GET /api/sessions/archived`. */
export type ClosedChat = { id: string; cwd: string; archivedAt: number; chair_provider: string | null };

/** `GET /api/chats/importable`: a chat on another adapter that the app can show. */
export type ImportableChat = { id: string; workspace: string; title: string; provider: string | null; lastActivity: number; running: boolean };

export type Profile = { name: string; runners: { provider: string; model?: string }[] };

export type LifeState = {
  /** The New Workspace dialog, on this project. */
  newWorkspace: { project: string } | null;
  create: Create | null;
  project: "create" | "import" | null;
  /** The plain folder that Import asks about: Git or a plain folder. */
  importChoice: string | null;
  /** The path that the Project dialog opens with. */
  projectPath: string;
  /** The folder that Create found already on disk, so the dialog offers Import. */
  existing: string | null;
  setupScript: { project: string; title: string } | null;
  /** The chat that Switch model continues. */
  switchModel: string | null;
  /** Each chat's switch, by its session; a switch blocks only its own chat. */
  handoffs: Record<string, Handoff>;
  confirm: Confirm | null;
  closed: boolean;
  imports: boolean;
};

// ponytail: the dialog keeps the last 256 KiB of setup output; a log view if scripts get chatty.
const OUTPUT_CAP = 256 * 1024;

/** The first prompt of a chat started from an issue: its title, then its body. */
export const issuePrompt = (issue: Issue) => `Issue #${issue.number}: ${issue.title}${issue.body.trim() ? `\n\n${issue.body.trim()}` : ""}`;

/** Applies one `setup` frame. A run follows the first workspace that starts while a create waits,
 *  because the create reply names the workspace only after setup ends. */
export function setupFrame(run: SetupRun | null, frame: Record<string, unknown>): SetupRun | null {
  const workspace = String(frame.workspace);
  if (!run) return frame.state === "running" ? { workspace, state: "running", output: "", exitCode: null, error: null } : null;
  if (run.workspace !== workspace) return run;
  if (frame.state === "output") {
    const output = run.output + String(frame.text ?? "");
    return { ...run, output: output.length > OUTPUT_CAP ? output.slice(-OUTPUT_CAP) : output };
  }
  if (frame.state === "completed" || frame.state === "failed" || frame.state === "timeout")
    return { ...run, state: frame.state, exitCode: typeof frame.exit_code === "number" ? frame.exit_code : null, error: frame.error ? String(frame.error) : null };
  return run;
}

/** The setup line under a kept workspace: what failed, and that the folder stays. */
export function setupFailure(run: SetupRun): string {
  const why = run.state === "timeout" ? "timed out" : run.exitCode !== null ? `failed with exit code ${run.exitCode}` : `failed: ${run.error ?? "unknown error"}`;
  return `The setup script ${why}. The workspace was kept${run.workspace ? ` at ${run.workspace}` : ""}.`;
}
