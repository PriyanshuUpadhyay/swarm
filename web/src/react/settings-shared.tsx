// Shared settings controls, labels, and state hooks.

import { useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
import type { App } from "../core/app";
import type { SettingsState } from "../core/settings";

const FIELD = "h-8 min-w-0 rounded-md border border-border-strong bg-background px-2 outline-none focus:border-ring disabled:opacity-60";
const PROVIDER_LABEL: Record<string, string> = { claude: "Claude", codex: "Codex", agy: "Gemini", pi: "Pi" };
const label = (provider: string) => PROVIDER_LABEL[provider] ?? provider;

function useSettings<K extends keyof SettingsState>(app: App, key: K): SettingsState[K] {
  return useSyncExternalStore(app.settings.subscribe, () => app.settings.state[key]);
}

/** Seconds since 1970, again each `ms`. */
function useNow(ms: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms]);
  return now;
}

function Row({ title, detail, htmlFor, live, children }: { title: string; detail?: string; htmlFor?: string; live?: boolean; children: ReactNode }) {
  return (
    <div className="flex items-center gap-4">
      <div className="min-w-0 flex-1">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="text-sm">
            {title}
          </label>
        ) : (
          <div className="text-sm">{title}</div>
        )}
        {detail && (
          <div className="text-xs break-words text-muted-foreground" role={live ? "status" : undefined}>
            {detail}
          </div>
        )}
      </div>
      {children}
    </div>
  );
}

/** `busy` ignores input but keeps focus, which `disabled` would drop to the page body. */
function Toggle({ label: name, checked, onChange, testid, id, disabled, busy }: { label: string; checked: boolean; onChange: (on: boolean) => void; testid: string; id?: string; disabled?: boolean; busy?: boolean }) {
  return <input type="checkbox" role="switch" className="toggle" id={id} disabled={disabled} aria-label={name} aria-busy={busy || undefined} aria-disabled={busy || undefined} data-testid={testid} checked={checked} onChange={(e) => !busy && onChange(e.target.checked)} />;
}


export { FIELD, label, Row, Segmented, Toggle, useSettings, useNow };

function Segmented<T extends string>({ label: name, value, options, onChange, testid }: { label: string; value: T; options: [T, string][]; onChange: (v: T) => void; testid: string }) {
  return (
    <div role="radiogroup" aria-label={name} className="view-switch m-0 w-56" data-testid={testid} onKeyDown={(e) => {
      // A radiogroup is one tab stop; the arrow keys move the choice (WAI-ARIA radio group).
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
      if (!step) return;
      e.preventDefault();
      const next = options[(options.findIndex(([v]) => v === value) + step + options.length) % options.length][0];
      onChange(next);
      e.currentTarget.querySelector<HTMLElement>(`[data-value="${next}"]`)?.focus();
    }}>
      {options.map(([v, text]) => (
        <button key={v} type="button" role="radio" aria-checked={value === v} tabIndex={value === v ? 0 : -1} data-value={v} onClick={() => value !== v && onChange(v)}>
          {text}
        </button>
      ))}
    </div>
  );
}
