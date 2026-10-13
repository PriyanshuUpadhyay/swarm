// The sidebar, tab strip and palette on the Rust view model (P2a, ADR 0049 and 0054). Rust owns the
// tree, its order, names, folds and fields; this module only lays the wire out for rendering and keys.

import type { Snapshot } from "./api";
import { fuzzySearchItems } from "./fuzzy-search";

export type Status = "ended" | "done" | "working" | "failed" | "waiting";
export type Field = { field: string; text: string; status?: Status };
export type Selection = { workspace: string | null; chat: string | null; agentSession: string | null; agent: string | null };

export type SidebarRow = {
  id: string;
  kind: "workspace" | "chat" | "child" | "more";
  depth: number;
  parentId: string | null;
  expanded: boolean;
  hasChildren: boolean;
  childrenSummary: string | null;
  title: string;
  detail: string;
  status: Status | null;
  counts: { status: Status; count: number }[];
  age: string | null;
  help: string;
  pinned: boolean;
  archived: boolean;
  missing: boolean;
  newChatEnabled: boolean;
  dimmed: boolean;
  fields: Field[];
  selection: Selection;
};

export type Section = {
  id: string;
  kind: "pinned" | "project";
  path: string | null;
  collapseId: string;
  expanded: boolean;
  title: string;
  status: Status | null;
  fields: Field[];
  rows: SidebarRow[];
};

export type TreeName = "workspaces" | "archived";

export type TreeView = {
  version: number;
  revision: string;
  view: TreeName;
  sections: Section[];
  selectedRow: string | null;
  archivedStatus: Status | null;
  choicesError: string | null;
};

export type Tab = {
  /** The stable chat key; a continued chat keeps it. */
  id: string;
  /** The chat's current session. */
  session: string;
  title: string;
  status: Status | null;
  badge: string | null;
  canClose: boolean;
  canHide: boolean;
  pending: "starting" | "failed" | "closing" | null;
  fields: Field[];
  children: { count: number; waiting: number; firstWaiting: string | null; text: string } | null;
};

/** A workspace's tab strip. Tab groups are not shown (ADR 0076), so `tabs` is the owner's order. */
export type Tabs = {
  workspace: string;
  tabs: Tab[];
  history: string[];
  selectedChat: string | null;
  previousRecent: string | null;
  nextRecent: string | null;
  stepTargets: Record<string, [string | null, string | null]>;
};

export type TabsView = { version: number; revision: string; view: Tabs };

/** One `POST /api/choices` operation (P2a). The app adds the revision. */
export type ChoiceOp =
  | { op: "pin" | "unpin"; workspace: string }
  | { op: "reorder_workspace"; workspace: string; target: string }
  | { op: "collapse" | "expand"; id: string }
  | { op: "expand_list"; workspace: string }
  | { op: "rename_workspace"; workspace: string; name: string }
  | { op: "rename_chat"; workspace: string; chat: string; name: string }
  | { op: "rename_project"; project: string; name: string }
  | { op: "open_tab" | "close_tab"; workspace: string; chat: string }
  | { op: "move_tab" | "step_tab"; workspace: string; chat: string; target: string }
  | { op: "selected_chat"; workspace: string; chat: string | null; recording_history?: boolean }
  | { op: "seen"; workspace: string; chat: string }
  | { op: "setup_script"; project: string; setup_script: string };

/** The new tree, with each row and section that did not change kept as the old object, so a
 *  memoized row skips its render on a refresh. */
export function shareTree(old: TreeView | null, next: TreeView): TreeView {
  if (!old) return next;
  const rows = new Map<string, SidebarRow>();
  const sections = new Map(old.sections.map((s) => [s.id, s]));
  for (const s of old.sections) for (const r of s.rows) rows.set(`${s.id}\n${r.id}`, r);
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  return {
    ...next,
    sections: next.sections.map((s) => {
      const kept = s.rows.map((r) => {
        const o = rows.get(`${s.id}\n${r.id}`);
        return o && same(o, r) ? o : r;
      });
      const o = sections.get(s.id);
      const section = { ...s, rows: kept };
      return o && o.rows.length === kept.length && o.rows.every((r, i) => r === kept[i]) && same({ ...o, rows: 0 }, { ...s, rows: 0 }) ? o : section;
    }),
  };
}

/** One sidebar line: a section header, or a tree row under it. */
export type NavRow = {
  id: string;
  kind: "pinned" | "project" | "workspace" | "chat" | "child" | "more";
  /** aria-level: 1 for a header, 2 for a workspace, and so on. */
  level: number;
  /** True or false for a row that folds; null for one that does not. */
  expanded: boolean | null;
  parentId: string | null;
  section: Section;
  /** Null for a header. */
  row: SidebarRow | null;
};

/** The lines in render order. The archived view has no folds. `folds` holds the owner's fold
 *  presses that the server has not confirmed yet, so a fold shows at once. */
export function navRows(tree: TreeView | null, folds: ReadonlyMap<string, boolean> = new Map()): NavRow[] {
  if (!tree) return [];
  const archive = tree.view === "archived";
  const out: NavRow[] = [];
  for (const section of tree.sections) {
    const expanded = archive ? true : (folds.get(section.collapseId) ?? section.expanded);
    out.push({ id: section.id, kind: section.kind, level: 1, expanded: archive ? null : expanded, parentId: null, section, row: null });
    if (!expanded) continue;
    // Rows under a folded row are skipped until the depth comes back up.
    let hideBelow = Infinity;
    for (const row of section.rows) {
      if (row.depth > hideBelow) continue;
      hideBelow = Infinity;
      const open = row.hasChildren ? (folds.get(row.id) ?? row.expanded) : null;
      if (open === false) hideBelow = row.depth;
      out.push({ id: row.id, kind: row.kind, level: row.depth + 2, expanded: open, parentId: row.parentId ?? section.id, section, row });
    }
  }
  return out;
}

/** The id a fold op takes: a header's collapse id, else the row id. */
export const foldId = (r: NavRow) => (r.row ? r.id : r.section.collapseId);

/** Every tree row with a chat, across both sections. */
export function chatRows(tree: TreeView | null): SidebarRow[] {
  return tree ? tree.sections.flatMap((s) => s.rows.filter((r) => r.kind === "chat")) : [];
}

/** The workspace a session belongs to: from the tree, else from the snapshot. */
export function workspaceOf(tree: TreeView | null, snapshot: Snapshot | null, session: string): string | null {
  for (const s of tree?.sections ?? []) for (const r of s.rows) if (r.selection.chat === session && r.selection.workspace) return r.selection.workspace;
  const row = snapshot?.sessions.find((s) => s.id === session);
  return row ? (row.workspace ?? row.cwd) : null;
}

/** The workspace row, wherever it is listed. */
export function workspaceRow(tree: TreeView | null, path: string | null): SidebarRow | undefined {
  if (!tree || !path) return undefined;
  for (const s of tree.sections) for (const r of s.rows) if (r.kind === "workspace" && r.id === path) return r;
  return undefined;
}

/** The project path of a workspace, from the snapshot's sessions in it. */
function projectOf(snapshot: Snapshot | null, workspace: string): string | null {
  return snapshot?.sessions.find((s) => (s.workspace ?? s.cwd) === workspace)?.project ?? null;
}

/** Move Up (-1) or Down (+1): the neighbour in the same project with the same pin state
 *  (WorkspaceNavigation.workspaceMoveTarget). A project section lists exactly those; the pinned
 *  section mixes projects, so its siblings are the pinned rows of the same project. */
export function moveTarget(tree: TreeView | null, snapshot: Snapshot | null, path: string, offset: -1 | 1): string | null {
  const section = tree?.sections.find((s) => s.rows.some((r) => r.kind === "workspace" && r.id === path));
  const row = section?.rows.find((r) => r.id === path);
  if (!section || !row || row.archived) return null;
  let siblings = section.rows.filter((r) => r.kind === "workspace" && !r.archived).map((r) => r.id);
  if (section.kind === "pinned") {
    const project = projectOf(snapshot, path);
    siblings = siblings.filter((p) => projectOf(snapshot, p) === project);
  }
  const i = siblings.indexOf(path);
  return siblings[i + offset] ?? null;
}

/** When a session last did something, in seconds: its newest message, agent state, or start. */
export function recency(snapshot: Snapshot | null, session: string): number | null {
  const s = snapshot?.sessions.find((x) => x.id === session);
  if (!s) return null;
  let at = s.created_at > 1e12 ? s.created_at / 1000 : s.created_at;
  if (typeof s.last_message_at === "number") at = Math.max(at, s.last_message_at);
  for (const a of snapshot?.agents[session] ?? []) if (typeof a.state_at_s === "number") at = Math.max(at, a.state_at_s);
  return at;
}

export type RecentChat = {
  session: string;
  title: string;
  project: string;
  workspace: string;
  workspacePath: string;
  age: string | null;
  status: Status | null;
  unread: boolean;
};

/** The listed chats across all projects, newest activity first, at most `limit` (HomeModel.recentWork). */
export function recentWork(tree: TreeView | null, snapshot: Snapshot | null, limit = 8): RecentChat[] {
  const seen = new Set<string>();
  const out: (RecentChat & { at: number })[] = [];
  for (const section of tree?.sections ?? [])
    for (const row of section.rows) {
      const session = row.selection.chat;
      if (row.kind !== "chat" || !session || seen.has(session)) continue;
      seen.add(session);
      const path = row.selection.workspace ?? "";
      const ws = workspaceRow(tree, path);
      // The pinned section mixes projects; its project is the one whose section also lists the workspace.
      const home = section.kind === "project" ? section : tree!.sections.find((s) => s.kind === "project" && s.rows.some((r) => r.id === path));
      const project = home?.title ?? projectOf(snapshot, path)?.split("/").pop() ?? "";
      out.push({
        session,
        title: row.title,
        project,
        workspace: ws?.title ?? path,
        workspacePath: path,
        age: row.age,
        status: row.status,
        unread: row.fields.some((f) => f.field === "unread"),
        at: recency(snapshot, session) ?? 0,
      });
    }
  out.sort((a, b) => b.at - a.at || (a.session < b.session ? -1 : 1));
  return out.slice(0, limit).map(({ at: _, ...r }) => r);
}

/** One palette result (PaletteItem). */
export type PaletteItem = {
  id: string;
  title: string;
  subtitle?: string;
  group: "action" | "workspace" | "chat" | "agent";
  shortcut?: string;
  status?: Status | null;
  recency?: number | null;
  disabledReason?: string | null;
  run: () => void;
};

export const PALETTE_GROUPS = [
  ["action", "Actions"],
  ["workspace", "Workspaces"],
  ["chat", "Chats"],
  ["agent", "Agents"],
] as const;

/** An empty query lists recent items newest first, then the actions; ties keep the input order. A
 *  query ranks every item by jean's fuzzy search, the title weighing twice the subtitle. */
export function rankPalette(items: PaletteItem[], query: string): PaletteItem[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) {
    const recent = items
      .map((item, i) => ({ item, i }))
      .filter((x) => x.item.recency != null)
      .sort((a, b) => b.item.recency! - a.item.recency! || a.i - b.i)
      .map((x) => x.item);
    return [...recent, ...items.filter((i) => i.group === "action" && i.recency == null)];
  }
  const found = fuzzySearchItems(
    items.map((item) => ({ item, name: item.title, description: item.subtitle })),
    query.trim(),
    items.length,
  );
  return found.map((f) => f.item);
}

/** The palette's sections: Recent then Actions with no query, else the four groups in order. */
export function paletteSections(items: PaletteItem[], query: string): { title: string; items: PaletteItem[] }[] {
  const ranked = rankPalette(items, query);
  const sections = query.trim()
    ? PALETTE_GROUPS.map(([g, title]) => ({ title, items: ranked.filter((i) => i.group === g) }))
    : [
        { title: "Recent", items: ranked.filter((i) => i.recency != null) },
        { title: "Actions", items: ranked.filter((i) => i.recency == null) },
      ];
  return sections.filter((s) => s.items.length);
}

/** Workspaces and chats for the palette: pinned first, then the rest by activity
 *  (PaletteSource.workspaces). Titles carry the project, since the palette has no headers. */
export function paletteSources(tree: TreeView | null, snapshot: Snapshot | null) {
  const workspaces: { row: SidebarRow; project: string; at: number | null; chats: SidebarRow[] }[] = [];
  for (const section of tree?.sections ?? [])
    for (const row of section.rows) {
      if (row.kind !== "workspace" || workspaces.some((w) => w.row.id === row.id)) continue;
      const chats = section.rows.filter((r) => r.kind === "chat" && r.selection.workspace === row.id);
      const ats = chats.map((c) => (c.selection.chat ? recency(snapshot, c.selection.chat) : null)).filter((a): a is number => a != null);
      workspaces.push({ row, project: section.kind === "project" ? section.title : "", at: ats.length ? Math.max(...ats) : null, chats });
    }
  const byActivity = [...workspaces].sort((a, b) => (b.at ?? 0) - (a.at ?? 0) || (a.row.id < b.row.id ? -1 : 1));
  return [...byActivity.filter((w) => w.row.pinned), ...byActivity.filter((w) => !w.row.pinned)];
}
