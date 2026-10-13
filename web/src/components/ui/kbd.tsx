import type * as React from "react";
import { cn } from "../../lib/utils";

function Kbd({ className, ...props }: React.ComponentProps<"kbd">) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        "pointer-events-none inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-sm border border-border bg-surface px-1 font-sans text-2xs font-medium text-muted-foreground select-none",
        className,
      )}
      {...props}
    />
  );
}

function KbdGroup({ className, ...props }: React.ComponentProps<"span">) {
  return <span data-slot="kbd-group" className={cn("inline-flex items-center gap-0.5", className)} {...props} />;
}

export { Kbd, KbdGroup };
