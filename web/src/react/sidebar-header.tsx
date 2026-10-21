import { BlocksIcon, HouseIcon, MonitorIcon, MoonIcon, PlusIcon, SearchIcon, SunIcon } from "lucide-react";
import { type App } from "../core/app";
import { Button } from "../components/ui/button";
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
import { chooseTheme, useTheme, type Theme } from "./appearance";

export function SidebarHeader({ app }: { app: App }) {
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
          <DropdownMenuItem onSelect={() => app.life.openClosed()}>Archived chats…</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => app.life.openImports()}>Import chats…</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <button className="search-box" aria-label="Command palette" onClick={() => app.setPalette(true)}>
        <SearchIcon />
        <span className="flex-1 text-left">Search…</span>
        <Kbd>⌘K</Kbd>
      </button>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label="Skills" data-testid="skills-button" onClick={() => app.skills.show()}>
            <BlocksIcon />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Skills</TooltipContent>
      </Tooltip>
      <ThemeMenu app={app} />
    </div>
  );
}

const THEME_ICON = { system: MonitorIcon, light: SunIcon, dark: MoonIcon };

function ThemeMenu({ app }: { app: App }) {
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
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => chooseTheme(app, v as Theme)}>
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
