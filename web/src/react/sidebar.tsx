import { ChevronRightIcon, FolderGit2Icon, MonitorIcon, MoonIcon, SearchIcon, SunIcon, TriangleAlertIcon } from "lucide-react";
import { useRef, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import type { TreeRow } from "../core/tree";
import { Button } from "../components/ui/button";
import { ContextMenu, ContextMenuTrigger } from "../components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../components/ui/dropdown-menu";
import { Kbd } from "../components/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { RowHints, RowMenu } from "./row-overlays";
import { StatusGlyph } from "./status";
import { setTheme, useTheme, type Theme } from "./theme";

export function Sidebar({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  const listRef = useRef<HTMLDivElement>(null);
  const { rows, selected, loaded, error } = view;
  return (
    <div className="flex h-full min-w-0 flex-col bg-sidebar">
      <SidebarHeader app={app} />
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={listRef}
            className="min-h-0 flex-1 overflow-y-auto pt-1 pb-3 outline-none"
            role="tree"
            aria-label="Chats"
            data-testid="sidebar"
            data-loaded={String(loaded)}
          >
            {rows.map((row) => (
              <TreeRowView key={row.id} app={app} row={row} selected={row.id === selected} />
            ))}
            {!loaded && !error && <SidebarSkeleton />}
            {loaded && !error && rows.length === 0 && (
              <SidebarNote title="No chats yet" body="Start one with swarm launch in a project folder." />
            )}
            {error && rows.length === 0 && <SidebarNote error title="Could not load chats" body={error} />}
          </div>
        </ContextMenuTrigger>
        <RowMenu app={app} listRef={listRef} />
      </ContextMenu>
      <RowHints app={app} listRef={listRef} />
      {error && rows.length > 0 && (
        <div role="status" className="flex items-center gap-2 border-t border-border px-3 py-2 text-xs text-muted-foreground">
          <TriangleAlertIcon className="size-3.5 shrink-0 text-failed" />
          <span className="truncate" title={error}>
            Offline. Showing the last list.
          </span>
        </div>
      )}
    </div>
  );
}

function TreeRowView({ app, row, selected }: { app: App; row: TreeRow; selected: boolean }) {
  return (
    <div
      className={`trow d${row.depth}`}
      role="treeitem"
      data-testid="tree-row"
      data-kind={row.kind}
      data-id={row.id}
      aria-level={row.depth}
      aria-expanded={row.expanded ?? undefined}
      aria-selected={selected}
      tabIndex={selected ? 0 : -1}
      onClick={() => app.clickRow(row.id)}
    >
      {row.expanded === null ? <span className="chev" /> : <ChevronRightIcon className="chev" data-open={row.expanded} />}
      {row.kind === "workspace" && <FolderGit2Icon className="icon" />}
      <span className="label">{row.label}</span>
      <StatusGlyph status={row.state} />
    </div>
  );
}

function SidebarHeader({ app }: { app: App }) {
  return (
    <div className="flex h-10 shrink-0 items-center gap-1 pr-2 pl-3">
      <span className="flex-1 text-2xs font-medium tracking-wide text-faint-foreground uppercase">Workspaces</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label="Command palette" onClick={() => app.setPalette(true)}>
            <SearchIcon />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          Command palette <Kbd>⌘K</Kbd>
        </TooltipContent>
      </Tooltip>
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
      <p className="mt-1 text-xs break-words text-muted-foreground">{body}</p>
    </div>
  );
}
