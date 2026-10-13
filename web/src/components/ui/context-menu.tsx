import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import type * as React from "react";
import { cn } from "../../lib/utils";
import { menuContent, menuItem, menuLabel, menuSeparator, menuShortcut } from "./menu-styles";

function ContextMenu(props: React.ComponentProps<typeof ContextMenuPrimitive.Root>) {
  return <ContextMenuPrimitive.Root data-slot="context-menu" {...props} />;
}

function ContextMenuTrigger(props: React.ComponentProps<typeof ContextMenuPrimitive.Trigger>) {
  return <ContextMenuPrimitive.Trigger data-slot="context-menu-trigger" {...props} />;
}

function ContextMenuContent({ className, ...props }: React.ComponentProps<typeof ContextMenuPrimitive.Content>) {
  return (
    <ContextMenuPrimitive.Portal>
      <ContextMenuPrimitive.Content data-slot="context-menu-content" className={cn(menuContent, className)} {...props} />
    </ContextMenuPrimitive.Portal>
  );
}

function ContextMenuItem({ className, ...props }: React.ComponentProps<typeof ContextMenuPrimitive.Item>) {
  return <ContextMenuPrimitive.Item data-slot="context-menu-item" className={cn(menuItem, className)} {...props} />;
}

function ContextMenuLabel({ className, ...props }: React.ComponentProps<typeof ContextMenuPrimitive.Label>) {
  return <ContextMenuPrimitive.Label data-slot="context-menu-label" className={cn(menuLabel, className)} {...props} />;
}

function ContextMenuSeparator({ className, ...props }: React.ComponentProps<typeof ContextMenuPrimitive.Separator>) {
  return <ContextMenuPrimitive.Separator data-slot="context-menu-separator" className={cn(menuSeparator, className)} {...props} />;
}

function ContextMenuShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return <span data-slot="context-menu-shortcut" className={cn(menuShortcut, className)} {...props} />;
}

export { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator, ContextMenuShortcut, ContextMenuTrigger };
