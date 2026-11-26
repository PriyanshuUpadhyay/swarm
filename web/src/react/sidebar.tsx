// The owner can open, pick, fold and drag rows of one server-projected list.

import { ArchiveRestoreIcon, TriangleAlertIcon } from "lucide-react";
import { useRef, useState, useSyncExternalStore, type DragEvent, type MouseEvent } from "react";
import type { App } from "../core/app";
import { archivedSession, currentRow, workspaceRow, type SidebarRow } from "../core/nav";
import { canStartIn } from "../core/new-chat";
import { Button } from "../components/ui/button";
import { ContextMenu, ContextMenuTrigger } from "../components/ui/context-menu";
import { Kbd } from "../components/ui/kbd";
import { RowHints, RowMenu } from "./row-overlays";
import { RowBar, treeTarget } from "./row-actions";
import { StatusGlyph } from "./status";
import { RowView } from "./sidebar-row";
import { SidebarHeader } from "./sidebar-header";

export function Sidebar({ app }: { app: App }) {
  return (
    <div className="flex h-full min-w-0 flex-col bg-sidebar">
      <SidebarHeader app={app} />
      <WorkspaceList app={app} />
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
  const dragged = useRef<SidebarRow | null>(null);
  const [dragging, setDragging] = useState(false);
  const archive = treeName === "archived";
  const hasPinned = tree?.sections.some((s) => s.kind === "pinned") ?? false;
  const current = currentRow(rows, openId, workspace, archive ? null : tree);

  const onClick = (e: MouseEvent) => {
    if ((e.target as Element).closest("a")) return;
    const el = (e.target as Element).closest<HTMLElement>('[data-testid="tree-row"]');
    const row = el && rows.find((r) => r.id === el.dataset.id);
    if (!row) return;
    const action = (e.target as Element).closest("[data-action]")?.getAttribute("data-action");
    if (action === "toggle") app.toggle(row.id);
    else if (action === "unarchive") app.unarchive(archivedSession(row.id));
    else if (action === "new-chat" && row.row) app.newChat.open(row.row.id);
    else if (action === "new-chat-project") app.newChat.open(row.section.path ?? undefined);
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
            className="min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-3 outline-none"
            role="tree"
            aria-multiselectable
            aria-label={archive ? "Archived workspaces" : "Chats"}
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
              <RowView key={row.id} row={row} selected={row.id === selected} marked={marked.size ? marked.has(row.id) : null} current={row.id === current}
                open={!archive && row.kind === "chat" && row.row?.selection?.chat === openId}
                facts={workspaceRow(tree, row.row?.selection?.workspace ?? null)} archive={archive} canNewIn={!row.row && canStartIn(tree, row.section.path)} />
            ))}
            {!loaded && !error && <SidebarSkeleton />}
            {loaded && rows.length === 0 && <SidebarNote title={archive ? "No archived workspaces" : "No projects yet"} body={archive ? "" : "Import or create a project from the + menu above."} />}
            {error && rows.length === 0 && <SidebarNote error title="Could not load the workspaces" body={error} />}
            <RowBar listRef={listRef} select={'[data-testid="tree-row"]'} target={treeTarget(app)} />
          </div>
        </ContextMenuTrigger>
        <RowMenu app={app} listRef={listRef} />
      </ContextMenu>
      <RowHints app={app} listRef={listRef} />
      {(error || tree?.choicesError) && rows.length > 0 && (
        <div role="status" className="flex items-center gap-2 border-t border-border px-3 py-2 text-xs text-muted-foreground">
          <TriangleAlertIcon className="size-3.5 shrink-0 text-failed" />
          <span className="truncate" title={error ?? tree?.choicesError ?? ""}>
            {error ? "Offline. Showing the last list." : `Choices: ${tree!.choicesError}`}
          </span>
        </div>
      )}
      <div className="flex shrink-0 flex-wrap items-center gap-x-1 border-t border-border px-2 py-1.5">
        <Button variant="ghost" size="sm" data-testid="archive-toggle" onClick={() => app.setTreeName(archive ? "workspaces" : "archived")}>
          <ArchiveRestoreIcon />
          {archive ? "Back to chats" : "Archive"}
          {!archive && tree?.archivedStatus && <StatusGlyph status={tree.archivedStatus} />}
        </Button>
        {archive && <Button variant="ghost" size="sm" onClick={() => app.life.openClosed()}>Archived chats…</Button>}
        <span className="ml-auto truncate pr-1 text-xs text-muted-foreground" data-testid="row-bar-hint">
          Hold <Kbd>⌘</Kbd> for actions
        </span>
      </div>
    </>
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
    <div role="status" className="mx-3 mt-4 rounded-xl border border-dashed border-border px-3 py-4 text-center">
      <div className="flex items-center justify-center gap-1.5 text-sm font-medium text-foreground">
        {error && <TriangleAlertIcon className="size-3.5 text-failed" />}
        {title}
      </div>
      {body && <p className="mt-1 text-xs break-words text-muted-foreground">{body}</p>}
    </div>
  );
}
