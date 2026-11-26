import type * as React from "react";
import { cn } from "../../lib/utils";

function Kbd({ className, ...props }: React.ComponentProps<"kbd">) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        "pointer-events-none inline-flex h-5 min-w-5 items-center justify-center rounded-sm border border-border bg-surface px-1 font-mono text-xs text-muted-foreground select-none",
        className,
      )}
      {...props}
    />
  );
}

export { Kbd };
