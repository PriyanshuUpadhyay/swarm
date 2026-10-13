// The sidebar on the Rust tree (P2a) in jean's look: a Workspaces / Recent switch, the project tree,
// and one Recent list across projects. Rows are plain elements; one context menu, tooltip, hover card
// and new-chat menu serve the tree (decision 3). Drags use the browser's own drag and drop.

import { ArchiveRestoreIcon, ChevronRightIcon, GitPullRequestIcon, GripVerticalIcon, HouseIcon, MonitorIcon, MoonIcon, PinIcon, PlusIcon, SearchIcon, SunIcon, TriangleAlertIcon } from "lucide-react";
import { memo, useRef, useState, useSyncExternalStore, type DragEvent, type MouseEvent } from "react";
import { SIDEBAR_VIEWS, type App } from "../core/app";
import type { Field, NavRow, RecentChat, SidebarRow } from "../core/nav";
import { Button } from "../components/ui/button";
import { ContextMenu, ContextMenuTrigger } from "../components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../components/ui/dropdown-menu";
import { Kbd } from "../components/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { NewChatMenu, RowHints, RowMenu, type NewChatAnchor } from "./row-overlays";
import { StatusGlyph } from "./status";
import { setTheme, useTheme, type Theme } from "./theme";

export function Sidebar({ app }: { app: App }) {
  const sidebarView = useSyncExternalStore(app.subscribe, () => app.view.sidebarView);
  return (
    <div className="flex h-full min-w-0 flex-col bg-sidebar">
      <SidebarHeader app={app} />
      <ViewSwitch app={app} current={sidebarView} />
      {sidebarView === 2 ? <RecentList app={app} /> : <WorkspaceList app={app} />}
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

  const onClick = (e: MouseEvent) => {
    const el = (e.target as Element).closest<HTMLElement>('[data-testid="tree-row"]');
    const row = el && rows.find((r) => r.id === el.dataset.id);
    if (!row) return;
    const action = (e.target as Element).closest("[data-action]")?.getAttribute("data-action");
    if (action === "toggle") app.toggle(row.id);
    else if (action === "new-chat" && row.row) newChat.current?.({ rect: (e.target as Element).closest("[data-action]")!.getBoundingClientRect(), workspace: row.row });
    else if (action === "new-workspace") app.life.newWorkspace(row.section.path ?? undefined);
    else app.clickRow(row.id);
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
              <RowView key={row.id} row={row} selected={row.id === selected} current={isCurrent(row, openId, workspace)} archive={archive} />
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

/** The open chat's row, or with no chat open, the selected workspace's. */
const isCurrent = (row: NavRow, openId: string | null, workspace: string | null) =>
  openId ? row.kind === "chat" && row.row?.selection.chat === openId : row.kind === "workspace" && row.id === workspace;

/** One sidebar line. Memoized on its wire object, so a key move renders only two rows. */
const RowView = memo(function RowView({ row, selected, current, archive }: { row: NavRow; selected: boolean; current: boolean; archive: boolean }) {
  const r = row.row;
  const common = {
    role: "treeitem",
    "data-testid": "tree-row",
    "data-kind": row.kind,
    "data-id": row.id,
    "aria-level": row.level,
    "aria-expanded": row.expanded ?? undefined,
    "aria-selected": selected,
    "aria-current": current || undefined,
    tabIndex: selected ? 0 : -1,
  } as const;
  const chev = (action = true) =>
    row.expanded === null ? <span className="chev" /> : <ChevronRightIcon className="chev" data-open={row.expanded} data-action={action ? "toggle" : undefined} />;

  if (!r) {
    const s = row.section;
    const folded = row.expanded === false;
    return (
      <div {...common} className="trow head" data-drop={s.kind === "pinned" ? "pin" : undefined} title={s.path ?? undefined}>
        <div className="tline">
          {s.kind === "pinned" ? (
            <>
              <PinIcon className="avatar-icon" />
              <span className="label">Pinned</span>
              {folded && s.status && <StatusGlyph status={s.status} />}
            </>
          ) : (
            <>
              <span className="avatar" aria-hidden>
                {s.title.slice(0, 1)}
              </span>
              {s.fields.map((f, i) =>
                f.field === "status" ? folded && s.status ? <StatusGlyph key={i} status={s.status} /> : null : <FieldView key={i} f={f} row={null} />,
              )}
            </>
          )}
          {chev()}
          <span className="flex-1" />
          {s.kind === "project" && !archive && (
            <button className="row-btn" data-action="new-workspace" aria-label={`New workspace in ${s.title}`} title="New workspace">
              <PlusIcon />
            </button>
          )}
        </div>
      </div>
    );
  }

  // Steps, and a chat's children, start a second line (SidebarRowView.lines).
  const lines: Field[][] = [[]];
  for (const f of r.fields) {
    if ((f.field === "steps" || (f.field === "children" && r.kind === "chat")) && lines[lines.length - 1].length) lines.push([]);
    lines[lines.length - 1].push(f);
  }
  if (!r.fields.length) lines[0].push({ field: "title", text: r.title });
  const canNew = r.kind === "workspace" && r.newChatEnabled && !r.archived;
  return (
    <div
      {...common}
      className="trow"
      data-session={r.kind === "chat" ? (r.selection.chat ?? undefined) : undefined}
      data-dimmed={r.dimmed || undefined}
      draggable={r.kind === "workspace" && !r.archived && !archive ? true : undefined}
      style={{ paddingLeft: 12 + r.depth * 14 }}
    >
      {lines.map((fields, i) => (
        <div key={i} className={i ? "tline sub" : "tline"}>
          {i === 0 && (r.kind === "workspace" || r.kind === "chat") && r.hasChildren ? chev() : <span className="chev" />}
          {fields.map((f, j) => (
            <FieldView key={j} f={f} row={r} />
          ))}
          <span className="flex-1" />
          {i === 0 && r.kind === "workspace" && !r.archived && !archive && <GripVerticalIcon className="grip" aria-hidden data-tip="Drag to reorder or pin workspace" />}
          {i === 0 && canNew && (
            <button className="row-btn" data-action="new-chat" aria-label={`New chat in ${r.title}`}>
              <PlusIcon />
            </button>
          )}
        </div>
      ))}
    </div>
  );
});

/** One row field (RowFieldLabel). Hovering a row swaps its age for per-status counts, in CSS. */
function FieldView({ f, row }: { f: Field; row: SidebarRow | null }) {
  if (f.status) return <StatusGlyph status={f.status} />;
  switch (f.field) {
    case "title":
      return <span className="label">{f.text}</span>;
    case "unread":
      return <span className="unread" role="img" aria-label="Unread" data-tip="Unread" />;
    case "children":
      return row?.kind === "chat" && row.hasChildren ? (
        <span className="fld link" data-action="toggle">
          {f.text}
        </span>
      ) : (
        <span className="fld">{f.text}</span>
      );
    case "steps":
      return (
        <span className="fld" data-tip="The Runs view comes later">
          {f.text}
        </span>
      );
    case "age":
      return row?.counts.length ? (
        <>
          <span className="fld age swap">{f.text}</span>
          <span className="fld counts">
            {row.counts.map((c) => (
              <span key={c.status} className="inline-flex items-center gap-0.5">
                <StatusGlyph status={c.status} />
                {c.count}
              </span>
            ))}
          </span>
        </>
      ) : (
        <span className="fld">{f.text}</span>
      );
    case "pr":
      return (
        <span className="fld pr">
          <GitPullRequestIcon className="size-3" aria-hidden />
          {f.text}
        </span>
      );
    case "ci":
      return CI_STATUS[f.text] ? (
        <span className="fld ci">
          <StatusGlyph status={CI_STATUS[f.text]} />
          {f.text}
        </span>
      ) : (
        <span className="fld ci">{f.text}</span>
      );
    default:
      return <span className={`fld ${f.field}`}>{f.text}</span>;
  }
}

/** The row's CI text from Rust (`github_fields`) as a status glyph. */
const CI_STATUS: Record<string, "failed" | "working" | "done"> = { "CI failed": "failed", "CI pending": "working", "CI passed": "done" };

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

const RecentRow = memo(function RecentRow({ r, selected, current }: { r: RecentChat; selected: boolean; current: boolean }) {
  return (
    <div
      role="option"
      data-testid="recent-row"
      data-session={r.session}
      aria-selected={selected}
      aria-current={current || undefined}
      tabIndex={selected ? 0 : -1}
      title={r.workspacePath}
      className="recent-row"
    >
      <span className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">{r.title}</span>
        {r.unread && <span className="unread" role="img" aria-label="Unread" />}
        {r.status && <StatusGlyph status={r.status} />}
      </span>
      <span className="flex items-center gap-2 text-[11px]">
        <span className="min-w-0 flex-1 truncate">{r.project && r.project !== r.workspace ? `${r.project} · ${r.workspace}` : r.workspace}</span>
        <span className="shrink-0 text-[10px] tabular-nums">{r.age}</span>
      </span>
    </div>
  );
});

function SidebarHeader({ app }: { app: App }) {
  return (
    <div className="flex h-10 shrink-0 items-center gap-0.5 pr-2 pl-2">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label="Home" data-testid="home-button" onClick={() => app.showHome()}>
            <HouseIcon />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Home</TooltipContent>
      </Tooltip>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label="Add project">
            <PlusIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onSelect={() => app.life.openProject("create")}>Create Project…</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => app.life.openProject("import")}>Import Project…</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => app.life.newWorkspace()}>New Workspace…</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => app.life.openClosed()}>Recently closed…</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <button className="search-box" aria-label="Command palette" onClick={() => app.setPalette(true)}>
        <SearchIcon />
        <span className="flex-1 text-left">Search…</span>
        <Kbd>⌘K</Kbd>
      </button>
      <ThemeMenu />
    </div>
  );
}

const THEME_ICON = { system: MonitorIcon, light: SunIcon, dark: MoonIcon };

function ThemeMenu() {
  const theme = useTheme();
  const Icon = THEME_ICON[theme];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label="Appearance">
          <Icon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Appearance</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as Theme)}>
          <DropdownMenuRadioItem value="system">
            <MonitorIcon /> System
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="light">
            <SunIcon /> Light
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">
            <MoonIcon /> Dark
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
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
