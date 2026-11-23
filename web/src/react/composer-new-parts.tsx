// The small parts of the new chat composer's chip row and menus (ADR 0149).

import type { KeyboardEvent } from "react";
import type { App } from "../core/app";
import type { Run } from "../core/composer";
import { ErrorLine } from "./lifecycle-shared";
import { RunPicker, useRunCatalog } from "./run-picker";

/** ↑ ↓ Home End move the focus between a listbox's options. */
export function moveOption(e: KeyboardEvent<HTMLElement>) {
  const options = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="option"]')];
  const at = options.indexOf(document.activeElement as HTMLElement);
  const next = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: options.length - 1 }[e.key];
  if (next === undefined) return;
  e.preventDefault();
  options[Math.max(0, Math.min(options.length - 1, next))]?.focus();
}

export function RunMenu({ app, current, draft, setDraft }: { app: App; current: Run; draft: Run; setDraft: (run: Run) => void }) {
  const catalog = useRunCatalog(app, current.provider);
  return (
    <>
      <RunPicker app={app} catalog={catalog} current={current} draft={draft} setDraft={setDraft} />
      <ErrorLine text={catalog.error} />
    </>
  );
}

export function Chip({ testid, on, title, disabled, onClick, children }: { testid: string; on: boolean; title: string; disabled?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" data-testid={testid} aria-expanded={on} title={title} disabled={disabled} className="max-w-[320px] truncate rounded-md border border-border px-2 py-1 hover:bg-hover aria-expanded:bg-selected disabled:pointer-events-none" onClick={onClick}>
      {children}
    </button>
  );
}
