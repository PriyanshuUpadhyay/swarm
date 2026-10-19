import { type Row } from "./api";
import { type AgentInput, type Sent, type SendKey, type Usage } from "./composer";
import { MarkdownCache } from "./markdown";
import { type PaneCell } from "./panes";
import { FoldChoices, type Line, type Match } from "./transcript";
import { type NavRow, type RecentChat, type Tabs, type TreeName, type TreeView } from "./nav";

export const TAIL = 200;
export const LRU_SIZE = 4;
export const MAX_KEYS = 32;

/** The full text behind a capped preview, fetched on demand. */
export type FullText = { state: "loading" } | { state: "ready"; text: string; lines: string[] } | { state: "error"; error: string };

export type Chat = {
  /** The session id for the chair's chat, and `paneId` for a child agent's. */
  id: string;
  sessionId: string;
  /** "chair", or the child agent's id. */
  agent: string;
  key: string;
  /** False for a child's chat until its column first shows. */
  loaded: boolean;
  rows: Row[];
  /** The next older page ends here; 0 means no older rows. */
  older: number;
  end: number;
  state: "loading" | "ready" | "error";
  error: string | null;
  version: number;
  shift: boolean;
  loadingOlder: boolean;
  // Reader state and render caches; the LRU drops them.
  folds: FoldChoices;
  opened: Map<string, boolean>;
  full: Map<string, FullText>;
  md: MarkdownCache;
  built: { version: number; lines: Line[] } | null;
  usage: Usage;
  /** Typed texts, shown as queued rows until the log has them. */
  sent: Sent[];
  memo: { sig: unknown[]; pane: Pane } | null;
};

/** A failed chat action: the server's error text, and the same action again. */
export type Banner = { text: string; retry: () => void };

/** One chat's composer actions in flight. It outlives the chat's rows in the LRU. */
export type Input = {
  sending: boolean;
  stopping: boolean;
  pulling: boolean;
  answering: { prompt: string; choice: number } | null;
  alreadySent: boolean;
  banner: Banner | null;
  /** Bumped when the app changes the draft (a send or a pull-back), so the box reloads it. */
  draftRev: number;
};

export type Composer = {
  /** The chat id that every composer action takes. */
  id: string;
  agent: AgentInput;
  state: Input;
  /** The owner's queued rows at the end of the transcript. */
  queued: number;
  usage: Usage;
  sendKey: SendKey;
};

export type Find = { open: boolean; query: string; matches: Match[]; index: number; seq: number };

/** One transcript with its composer: the chair's chat, or a child agent's column. */
export type Pane = {
  /** The chat id; "" when no chat is open. */
  id: string;
  chatState: "none" | "loading" | "ready" | "error";
  chatError: string | null;
  lines: Line[];
  hasOlder: boolean;
  loadingOlder: boolean;
  shift: boolean;
  md: MarkdownCache | null;
  full: ReadonlyMap<string, FullText>;
  find: Find;
  composer: Composer | null;
};

/** The open chat's child agents beside it (ADR 0030). `focused`, `zoomed` and `reveal` are agent ids. */
export type Strip = {
  cells: PaneCell[];
  panes: ReadonlyMap<string, Pane>;
  focused: string | null;
  zoomed: string | null;
  /** The pane a key moved focus to; null shows the chat page. The strip scrolls to it. */
  reveal: string | null;
  /** Changes on every reveal, so a repeated reveal still scrolls. */
  revealSeq: number;
};

/** The chair's chat as `Pane` fields, and the rest of the window. */
export type View = Pane & {
  loaded: boolean;
  tree: TreeView | null;
  treeName: TreeName;
  error: string | null;
  /** The sidebar's lines; `selected` is the keyboard row among them. */
  rows: NavRow[];
  selected: string | null;
  /** Chat rows picked with ⌘-click or Shift-click; the row menu acts on all of them. */
  marked: ReadonlySet<string>;
  openId: string | null;
  /** The workspace whose tabs show; with no chat open, its landing shows. Null is Home. */
  workspace: string | null;
  /** The tab strip of `workspace`, once loaded. */
  tabs: Tabs | null;
  /** 1 Workspaces, 2 Recent (⌥⌘1-2). */
  sidebarView: number;
  /** Every listed chat across projects, newest activity first; `recentSelected` is its keyboard row. */
  recent: RecentChat[];
  recentSelected: string | null;
  paletteOpen: boolean;
  asking: Ask | null;
  strip: Strip;
};

/** What a transcript's rows call. `App` is the reader of the chair's chat; `App.reader` gives a child's. */
export type Reader = {
  toggleLine(line: Line): void;
  loadFull(previewId: string): Promise<void>;
  fullText(previewId: string): Promise<string>;
  /** A pasted image's preview URL. */
  imageUrl(path: string): Promise<string>;
  loadOlder(): Promise<void>;
  setFindQuery(query: string): void;
  stepFind(dir: number): void;
  closeFind(): void;
  /** Reuse on a sent prompt: puts its text in the composer `composerId`. */
  reuse(composerId: string, text: string): void;
};

/** A name the owner types: a rename. */
export type Ask = { title: string; value: string; save: (name: string) => void };

/** The sidebar views in ⌥⌘ order. */
export const SIDEBAR_VIEWS = ["Workspaces", "Recent"];

export const EMPTY: Line[] = [];
export const NO_FULL: ReadonlyMap<string, FullText> = new Map();
/** The chat id of a child agent's column. It is also the agent's stream key. */
export const paneId = (sessionId: string, agent: string) => `${sessionId}:${agent}`;
export const DRAFT_PREFIX = "composer.draft.";
/** The window event that moves the chat's transcript to the owner's previous (-1) or next (1) prompt. */
export const PROMPT_JUMP = "swarm:prompt-jump";
export const PICKS_PREFIX = "composer.skills.";
export const HISTORY_PREFIX = "composer.history.";
export const STASH_PREFIX = "composer.stash.";
export const NO_FIND: Find = { open: false, query: "", matches: [], index: -1, seq: 0 };
/** The last line while the agent runs. A turn can log nothing for minutes, such as Claude's /compact. */
export const WORKING: Line = {
  id: "working",
  row: { id: "working", kind: "notice", system_kind: "working", text: "Working…", order: [Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER] },
  fold: null,
  open: false,
};
export const NO_PANE: Pane = {
  id: "",
  chatState: "none",
  chatError: null,
  lines: EMPTY,
  hasOlder: false,
  loadingOlder: false,
  shift: false,
  md: null,
  full: NO_FULL,
  find: NO_FIND,
  composer: null,
};
export const NO_CELLS: PaneCell[] = [];
export const message = (err: unknown) => (err instanceof Error ? err.message : String(err));
/** A live chat row, the rows that End and Archive act on. */
export const markable = (r: NavRow) => r.kind === "chat" && !r.row?.archived && !!r.row?.selection?.chat;
export const readStore = (key: string): string | null => {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
};
export const writeStore = (key: string, value: string) => {
  try {
    if (value) localStorage.setItem(key, value);
    else localStorage.removeItem(key);
  } catch {
    // Keep working without persistence.
  }
};
