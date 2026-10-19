// The sidebar on the Rust tree (P2a) in jean's look: a Workspaces / Recent switch, the project tree,
// and one Recent list across projects. Rows are plain elements; one context menu, tooltip, hover card
// and new-chat menu serve the tree (decision 3). Drags use the browser's own drag and drop.

import { ArchiveRestoreIcon, TriangleAlertIcon } from "lucide-react";
import { useRef, useState, useSyncExternalStore, type DragEvent, type MouseEvent } from "react";
import { SIDEBAR_VIEWS, type App } from "../core/app";
import { archivedSession, currentRow, type SidebarRow } from "../core/nav";
import { Button } from "../components/ui/button";
import { ContextMenu, ContextMenuTrigger } from "../components/ui/context-menu";
import { NewChatMenu, RowHints, RowMenu, type NewChatAnchor } from "./row-overlays";
import { StatusGlyph } from "./status";
import { UsageMeters } from "./settings";
import { RowView, RecentRow } from "./sidebar-row";
import { SidebarHeader } from "./sidebar-header";

export function Sidebar({ app }: { app: App }) {
  const sidebarView = useSyncExternalStore(app.subscribe, () => app.view.sidebarView);
  return (
    <div className="flex h-full min-w-0 flex-col bg-sidebar">
      <SidebarHeader app={app} />
      <ViewSwitch app={app} current={sidebarView} />
      {sidebarView === 2 ? <RecentList app={app} /> : <WorkspaceList app={app} />}
      <UsageMeters app={app} />
    </div>
  );
}

/** jean's Projects / Recent switch (⌥⌘1, ⌥⌘2). */
function ViewSwitch({ app, current }: { app: App; current: number }) {
  return (
    <div role="tablist" aria-label="Sidebar view" className="view-switch">
      {SIDEBAR_VIEWS.map((name, i) => (
        <button key={name} role="tab" aria-selected={current === i + 1} data-testid={`view-${name.toLowerCase()}`} title={`${name} (⌥⌘${i + 1})`} onClick={() => app.setSidebarView(i + 1)}>
          {name}
        </button>
      ))}
    </div>
  );
}

function WorkspaceList({ app }: { app: App }) {
  // Only the tree's own fields: a rows frame in an open chat or pane must not render the list.
  const rows = useSyncExternalStore(app.subscribe, () => app.view.rows);
  const selected = useSyncExternalStore(app.subscribe, () => app.view.selected);
  const marked = useSyncExternalStore(app.subscribe, () => app.view.marked);
  const loaded = useSyncExternalStore(app.subscribe, () => app.view.loaded);
  const error = useSyncExternalStore(app.subscribe, () => app.view.error);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const treeName = useSyncExternalStore(app.subscribe, () => app.view.treeName);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const workspace = useSyncExternalStore(app.subscribe, () => app.view.workspace);
  const listRef = useRef<HTMLDivElement>(null);
  const newChat = useRef<((anchor: NewChatAnchor) => void) | null>(null);
  const dragged = useRef<SidebarRow | null>(null);
  const [dragging, setDragging] = useState(false);
  const archive = treeName === "archived";
  const hasPinned = tree?.sections.some((s) => s.kind === "pinned") ?? false;
  const current = currentRow(rows, openId, workspace);

  const onClick = (e: MouseEvent) => {
    const el = (e.target as Element).closest<HTMLElement>('[data-testid="tree-row"]');
    const row = el && rows.find((r) => r.id === el.dataset.id);
    if (!row) return;
    const action = (e.target as Element).closest("[data-action]")?.getAttribute("data-action");
    if (action === "toggle") app.toggle(row.id);
    else if (action === "unarchive") app.unarchive(archivedSession(row.id));
    else if (action === "new-chat" && row.row) newChat.current?.({ rect: (e.target as Element).closest("[data-action]")!.getBoundingClientRect(), workspace: row.row });
    else if (action === "new-workspace") app.life.newWorkspace(row.section.path ?? undefined);
    else app.clickRow(row.id, e.metaKey || e.ctrlKey ? "toggle" : e.shiftKey ? "range" : null);
  };

  // A drop target: the Pinned header or the pin zone, or another workspace of the same section.
  const dropTarget = (e: DragEvent): { el: HTMLElement; pin: boolean; target?: string } | null => {
    const source = dragged.current;
    if (!source) return null;
    const el = (e.target as Element).closest<HTMLElement>('[data-drop="pin"], [data-testid="tree-row"][data-kind="workspace"]');
    if (!el) return null;
    if (el.dataset.drop === "pin") return source.pinned ? null : { el, pin: true };
    const target = rows.find((r) => r.id === el.dataset.id);
    const from = rows.find((r) => r.id === source.id);
    if (!target || target.id === source.id || target.section !== from?.section || target.row?.archived) return null;
    return { el, pin: false, target: target.id };
  };
  const clearMark = () => listRef.current?.querySelectorAll("[data-over]").forEach((n) => n.removeAttribute("data-over"));
  const dragProps = archive
    ? {}
    : {
        onDragStart: (e: DragEvent) => {
          const el = (e.target as Element).closest<HTMLElement>('[data-testid="tree-row"][data-kind="workspace"]');
          const row = el && rows.find((r) => r.id === el.dataset.id)?.row;
          if (!row || row.archived) return;
          dragged.current = row;
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", row.id);
          setDragging(true);
        },
        onDragOver: (e: DragEvent) => {
          const t = dropTarget(e);
          if (!t) return;
          e.preventDefault();
          if (!t.el.hasAttribute("data-over")) {
            clearMark();
            t.el.setAttribute("data-over", "true");
          }
        },
        onDrop: (e: DragEvent) => {
          const t = dropTarget(e);
          const source = dragged.current;
          clearMark();
          if (!t || !source) return;
          e.preventDefault();
          if (t.pin) void app.pin(source.id);
          else void app.choose({ op: "reorder_workspace", workspace: source.id, target: t.target! });
        },
        onDragEnd: () => {
          dragged.current = null;
          clearMark();
          setDragging(false);
        },
      };

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={listRef}
            className="min-h-0 flex-1 overflow-y-auto pt-1 pb-3 outline-none"
            role="tree"
            aria-multiselectable
            aria-label={archive ? "Archived workspaces" : "Workspaces"}
            data-testid="sidebar"
            data-loaded={String(loaded)}
            onClick={onClick}
            {...dragProps}
          >
            {dragging && !hasPinned && (
              <div data-drop="pin" data-testid="pin-zone" className="pin-zone">
                Drop here to pin
              </div>
            )}
            {rows.map((row) => (
              <RowView key={row.id} row={row} selected={row.id === selected} marked={marked.size ? marked.has(row.id) : null} current={row.id === current} archive={archive} />
            ))}
            {!loaded && !error && <SidebarSkeleton />}
            {loaded && rows.length === 0 && <SidebarNote title={archive ? "No archived workspaces" : "No projects yet"} body={archive ? "" : "Import or create a project from the + menu above."} />}
            {error && rows.length === 0 && <SidebarNote error title="Could not load the workspaces" body={error} />}
          </div>
        </ContextMenuTrigger>
        <RowMenu app={app} listRef={listRef} />
      </ContextMenu>
      <RowHints app={app} listRef={listRef} />
      <NewChatMenu app={app} open={newChat} />
      {(error || tree?.choicesError) && rows.length > 0 && (
        <div role="status" className="flex items-center gap-2 border-t border-border px-3 py-2 text-xs text-muted-foreground">
          <TriangleAlertIcon className="size-3.5 shrink-0 text-failed" />
          <span className="truncate" title={error ?? tree?.choicesError ?? ""}>
            {error ? "Offline. Showing the last list." : `Choices: ${tree!.choicesError}`}
          </span>
        </div>
      )}
      <div className="flex shrink-0 items-center border-t border-border px-2 py-1.5">
        <Button variant="ghost" size="sm" data-testid="archive-toggle" onClick={() => app.setTreeName(archive ? "workspaces" : "archived")}>
          <ArchiveRestoreIcon />
          {archive ? "Workspaces" : "Archived"}
          {!archive && tree?.archivedStatus && <StatusGlyph status={tree.archivedStatus} />}
        </Button>
      </div>
    </>
  );
}

/** Every listed chat across projects, newest activity first (jean's Recent list). j/k, arrows and
 *  Enter work here as in the tree. */
function RecentList({ app }: { app: App }) {
  const recent = useSyncExternalStore(app.subscribe, () => app.view.recent);
  const selected = useSyncExternalStore(app.subscribe, () => app.view.recentSelected);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const loaded = useSyncExternalStore(app.subscribe, () => app.view.loaded);
  const onClick = (e: MouseEvent) => {
    const session = (e.target as Element).closest<HTMLElement>('[data-testid="recent-row"]')?.dataset.session;
    if (session) app.clickRecent(session);
  };
  return (
    <div role="listbox" aria-label="Recent chats" data-testid="recent" tabIndex={selected ? -1 : 0} className="min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-3 outline-none" onClick={onClick}>
      {recent.map((r) => (
        <RecentRow key={r.session} r={r} selected={r.session === selected} current={r.session === openId} />
      ))}
      {loaded && recent.length === 0 && <SidebarNote title="No chats yet" body="Chats from every project show here, newest first." />}
    </div>
  );
}

const SKELETON = [
  [1, 40],
  [2, 64],
  [3, 72],
  [3, 56],
  [2, 48],
  [3, 68],
  [1, 36],
  [2, 58],
  [3, 62],
];

function SidebarSkeleton() {
  return (
    <div aria-hidden className="px-2 pt-1">
      {SKELETON.map(([depth, width], i) => (
        <div key={i} className="flex h-[26px] items-center" style={{ paddingLeft: (depth - 1) * 14 + 8 }}>
          <div className="h-2 rounded-full bg-selected" style={{ width: `${width}%` }} />
        </div>
      ))}
    </div>
  );
}

function SidebarNote({ title, body, error = false }: { title: string; body: string; error?: boolean }) {
  return (
    <div role="status" className="mx-3 mt-4 rounded-lg border border-dashed border-border-strong px-3 py-4 text-center">
      <div className="flex items-center justify-center gap-1.5 text-sm font-medium text-foreground">
        {error && <TriangleAlertIcon className="size-3.5 text-failed" />}
        {title}
      </div>
      {body && <p className="mt-1 text-xs break-words text-muted-foreground">{body}</p>}
    </div>
  );
}
