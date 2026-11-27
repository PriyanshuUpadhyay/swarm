import type { PatchLineWire, TextPreview } from "./api";

export type Layer = "staged" | "unstaged" | "untracked" | "conflicted" | "branch";
export type Mode = "local" | "branch";

/** One changed file. A file in both the staged and unstaged layers is two entries. Binary files and
 *  symlinks have null counts. */
export type ChangeFile = { path: string; status: string; layer: Layer; added: number | null; removed: number | null };
export type ChangeList = { files: ChangeFile[]; refs: string[]; comparison: string[]; identity: { head: string | null; branch: string | null } };
/** A capped patch (200 lines, 12,000 characters) with the full counts, or a notice for a binary file. */
export type FileDiff = {
  path: string;
  layer?: Layer;
  patch?: string;
  display?: { lines: PatchLineWire[] };
  is_truncated?: boolean;
  full_lines?: number;
  hidden_lines?: number;
  added: number | null;
  removed: number | null;
  notice?: string;
};
export type FilePreview = { path: string; preview?: TextPreview; notice?: string };

export type Check = {
  __typename?: string;
  name?: string;
  context?: string;
  workflowName?: string;
  status?: string;
  conclusion?: string | null;
  state?: string;
  detailsUrl?: string;
  targetUrl?: string;
};
export type PullRequest = {
  number: number;
  title: string;
  state: string;
  isDraft: boolean;
  baseRefName: string;
  headRefName: string;
  reviewDecision: string | null;
  url: string;
};
export type PrView = { pullRequest: PullRequest | null; checks: Check[] | null; differsFromLocalHead?: boolean };

export type Load<T> = { state: "idle" } | { state: "loading" } | { state: "ready"; data: T } | { state: "error"; error: string; capped?: boolean };

/** One draft comment on a diff: the file, the line range label, the quoted lines with their +/-
 *  marks, and the owner's text. `at` is the display line it shows under. */
export type DiffComment = { id: number; path: string; layer: Layer; lines: string; quoted: string[]; text: string; at: number };

export type CommentEditor = { anchor: number; cursor: number; text: string };

export type Tab = "changes" | "pr";

export type ChangesView = {
  open: boolean;
  tab: Tab;
  workspace: string | null;
  mode: Mode;
  base: string | null;
  list: Load<ChangeList>;
  /** `fileKey` of the file whose diff shows. */
  selected: string | null;
  /** Diff, or the read-only file. */
  showFile: boolean;
  diff: Load<FileDiff>;
  full: Load<FileDiff>;
  file: Load<FilePreview>;
  /** Drafts of this workspace, oldest first. They survive a file switch and a closed panel. */
  comments: DiffComment[];
  sending: boolean;
  pr: Load<PrView>;
};
