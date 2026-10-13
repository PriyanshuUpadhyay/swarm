// Shared by the context menu and the dropdown menu, so both read the same.

export const menuContent =
  "z-50 min-w-[180px] overflow-hidden rounded-lg border border-border-strong bg-popover p-1 text-popover-foreground shadow-panel outline-none";
export const menuItem =
  "relative flex h-7 cursor-default items-center gap-2 rounded-md px-2 text-base outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[highlighted]:bg-selected [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5 [&_svg]:text-muted-foreground";
export const menuLabel = "px-2 pt-1.5 pb-1 text-2xs font-medium tracking-wide text-faint-foreground uppercase";
export const menuSeparator = "-mx-1 my-1 h-px bg-border";
export const menuShortcut = "ml-auto pl-4 text-xs text-faint-foreground";
