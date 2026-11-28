import { PlusIcon, SearchIcon, SettingsIcon } from "lucide-react";
import { type App } from "../core/app";
import { Button } from "../components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "../components/ui/dropdown-menu";
import { Kbd } from "../components/ui/kbd";
import { Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { Glyph } from "./glyph";

export function SidebarHeader({ app }: { app: App }) {
  return (
    <div className="flex shrink-0 flex-col gap-3 px-3 pt-[9px] pb-3" data-testid="sidebar-head">
      <div className="flex items-center gap-0.5">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="sm" className="text-md font-semibold" data-testid="home-button" onClick={() => app.showHome()}>
              <Glyph aria-hidden />
              Swarm
            </Button>
          </TooltipTrigger>
          <TooltipContent>Home</TooltipContent>
        </Tooltip>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" className="ml-auto text-muted-foreground" aria-label="Add project">
              <PlusIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onSelect={() => app.life.openProject("create")}>Create Project…</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => app.life.openProject("import")}>Import Project…</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => app.life.openImports()}>Import chats…</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label="Settings" data-testid="settings-button" onClick={() => app.settings.show()}>
              <SettingsIcon />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Settings (⌘,)</TooltipContent>
        </Tooltip>
      </div>
      <Button type="button" onClick={() => app.newChat.open()}>New chat</Button>
      <button type="button" className="search-box" aria-label="Search" onClick={() => app.setPalette(true)}>
        <SearchIcon />
        <span className="flex-1 text-left">Search…</span>
        <Kbd>⌘K</Kbd>
      </button>
    </div>
  );
}
