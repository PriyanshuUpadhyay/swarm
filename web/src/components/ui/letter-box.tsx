import { cn } from "../../lib/utils";

function LetterBox({ letter, className }: { letter: string; className?: string }) {
  return <span className={cn("inline-flex size-4 shrink-0 items-center justify-center rounded-sm border border-border font-mono text-xs text-muted-foreground", className)}>{letter}</span>;
}

export { LetterBox };
