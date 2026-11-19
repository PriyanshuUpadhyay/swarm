import { useEffect, useMemo, useSyncExternalStore } from "react";
import type { codeRuns } from "./code-runs";

// The highlighter compiles WebAssembly at import. It loads in its own chunk after the first code
// block, so a failed load leaves plain code and never stops the app from starting.
let runs: typeof codeRuns | null = null;
let loading: Promise<void> | null = null;
const subs = new Set<() => void>();
const subscribe = (f: () => void) => (subs.add(f), () => subs.delete(f));

export function loadHighlighter(): Promise<void> {
  loading ??= import("./code-runs").then((m) => {
    runs = m.codeRuns;
    subs.forEach((f) => f());
  }, (error: unknown) => console.warn("Code colours are off:", error));
  return loading;
}

export function HighlightedCode({ text, lang }: { text: string; lang: string }) {
  const ready = useSyncExternalStore(subscribe, () => runs, () => runs);
  useEffect(() => void loadHighlighter(), []);
  const coloured = useMemo(() => ready?.(text, lang) ?? null, [ready, text, lang]);
  return <code className={`language-${lang}`}>{coloured ?? text}</code>;
}
