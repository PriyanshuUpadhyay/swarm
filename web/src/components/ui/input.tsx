import type { ComponentProps } from "react";
import { cn } from "../../lib/utils";

const inputClass = "h-8 min-w-0 rounded-lg border border-border bg-background px-2 outline-none focus:border-ring focus-visible:ring-2 focus-visible:ring-ring disabled:text-disabled-foreground";

function Input({ className, ...props }: ComponentProps<"input">) {
  return <input className={cn(inputClass, className)} {...props} />;
}

export { Input, inputClass };
