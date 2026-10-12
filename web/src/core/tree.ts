// Sidebar tree: project -> workspace -> chat, from the snapshot (ADR 0049).

import type { Agent, Session, Snapshot } from "./api";

export type RowState = "waiting" | "working" | "idle";
export type RowKind = "project" | "workspace" | "chat";

export type TreeNode = {
  id: string;
  kind: RowKind;
  label: string;
  title: string;
  state: RowState;
  children: TreeNode[];
  sessionId?: string;
};

export type TreeRow = {
  id: string;
  kind: RowKind;
  label: string;
  title: string;
  state: RowState;
  depth: number;
  expanded: boolean | null;
  parentId: string | null;
  sessionId?: string;
};

const RANK: Record<RowState, number> = { idle: 0, working: 1, waiting: 2 };

export function agentState(agent: Agent): RowState {
  return agent.state === "waiting" || agent.state === "working" ? agent.state : "idle";
}

/** Waiting beats working beats idle. */
export function rollUp(states: Iterable<RowState>): RowState {
  let best: RowState = "idle";
  for (const s of states) if (RANK[s] > RANK[best]) best = s;
  return best;
}

export function baseName(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function chatLabel(s: Session): string {
  const when = new Date(s.created_at > 1e12 ? s.created_at : s.created_at * 1000);
  const hhmm = Number.isNaN(when.getTime()) ? "" : ` ${when.toISOString().slice(5, 16).replace("T", " ")}`;
  return `${s.chair_provider ?? s.adapter ?? "chat"} ${s.id.slice(0, 8)}${hhmm}`;
}

const byPath = (a: TreeNode, b: TreeNode) => (a.title < b.title ? -1 : a.title > b.title ? 1 : 0);

/** Projects and workspaces keep path order; chats are newest first. The order never depends on activity. */
export function buildTree(snap: Snapshot): TreeNode[] {
  const projects = new Map<string, TreeNode>();
  const workspaces = new Map<string, TreeNode>();
  const newestFirst = [...snap.sessions].sort((a, b) => b.created_at - a.created_at || (a.id < b.id ? -1 : 1));
  for (const s of newestFirst) {
    const wsPath = s.workspace ?? s.cwd;
    const projPath = s.project ?? wsPath;
    let project = projects.get(projPath);
    if (!project) {
      project = { id: `p:${projPath}`, kind: "project", label: baseName(projPath), title: projPath, state: "idle", children: [] };
      projects.set(projPath, project);
    }
    let ws = workspaces.get(wsPath);
    if (!ws) {
      ws = { id: `w:${wsPath}`, kind: "workspace", label: baseName(wsPath), title: wsPath, state: "idle", children: [] };
      workspaces.set(wsPath, ws);
      project.children.push(ws);
    }
    const agents = snap.agents[s.id] ?? [];
    ws.children.push({
      id: `c:${s.id}`,
      kind: "chat",
      label: chatLabel(s),
      title: `${baseName(projPath)} / ${baseName(wsPath)} / ${chatLabel(s)}`,
      state: rollUp(agents.map(agentState)),
      children: [],
      sessionId: s.id,
    });
  }
  const roots = [...projects.values()].sort(byPath);
  for (const p of roots) {
    p.children.sort(byPath);
    for (const w of p.children) {
      w.state = rollUp(w.children.map((c) => c.state));
    }
    p.state = rollUp(p.children.map((w) => w.state));
  }
  return roots;
}

/** Visible rows in render order. A collapsed node hides its children. */
export function flatten(roots: TreeNode[], collapsed: ReadonlySet<string>): TreeRow[] {
  const out: TreeRow[] = [];
  const walk = (node: TreeNode, depth: number, parentId: string | null) => {
    const open = !collapsed.has(node.id);
    out.push({
      id: node.id,
      kind: node.kind,
      label: node.label,
      title: node.title,
      state: node.state,
      depth,
      expanded: node.children.length ? open : null,
      parentId,
      sessionId: node.sessionId,
    });
    if (open) for (const c of node.children) walk(c, depth + 1, node.id);
  };
  for (const r of roots) walk(r, 1, null);
  return out;
}

/** Every workspace node in render order, ignoring collapse. */
export function workspaces(roots: TreeNode[]): TreeNode[] {
  return roots.flatMap((p) => p.children);
}
