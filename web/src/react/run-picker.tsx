// The run picker: models of each CLI, the thinking level and the account. The chat's run chip
// and the new chat composer both show it (ADR 0110, ADR 0147).

import { useEffect, useId, useState, useSyncExternalStore, type Dispatch } from "react";
import type { App } from "../core/app";
import { accountCli, accountColor, chooseModel, cliName, levelWord, limitText, modelLevels, type Run } from "../core/composer";
import type { ModelInfo } from "../core/run-catalog";
import type { Account, ProviderInfo } from "../core/settings";
import { Button } from "../components/ui/button";
import { FIELD, message } from "./lifecycle-shared";
import { modelPickerKey } from "./model-picker";
import { meterTone } from "./usage-meters";

export type Catalog = { providers: ProviderInfo[]; models: Record<string, ModelInfo[] | string>; error: string | null };

/** The CLIs and their models. A failed CLI read still lists `provider`'s models. */
export function useRunCatalog(app: App, provider: string): Catalog {
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [models, setModels] = useState<Record<string, ModelInfo[] | string>>({});
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const read = (id: string) =>
      app.life.runs.models(id).then(
        (m) => live && setModels((all) => ({ ...all, [id]: m.models })),
        (err) => live && setModels((all) => ({ ...all, [id]: `Could not read the models: ${message(err)}` })),
      );
    app.life.runs.providers().then(
      ({ providers }) => {
        if (!live) return;
        setProviders(providers);
        for (const p of providers) void read(p.id);
      },
      (err) => {
        if (!live) return;
        // The chat's own CLI still lists its models, and a typed id still works.
        setError(`Could not read the CLIs: ${message(err)}`);
        void read(provider);
      },
    );
    return () => {
      live = false;
    };
  }, [app, provider]);

  return { providers, models, error };
}

/** The picked model's own entry, its levels, and its default level. */
export function runModel(catalog: Catalog, draft: Run) {
  const info = catalog.providers.find((p) => p.id === draft.provider);
  const list = catalog.models[draft.provider];
  const model = Array.isArray(list) ? list.find((m) => m.id === draft.model) : undefined;
  return { info, model, levels: modelLevels(info?.efforts ?? [], model?.efforts), defaultLevel: model?.default_effort ?? info?.default_effort, label: model?.label ?? draft.model ?? "" };
}

/** `current` is the run the picker starts from; `note` labels a CLI's group. */
export function RunPicker({ app, catalog, current, draft, setDraft, disabled = false, note }: { app: App; catalog: Catalog; current: Run; draft: Run; setDraft: Dispatch<Run>; disabled?: boolean; note?: (provider: string) => string | null }) {
  const [accounts, setAccounts] = useState<Pick<Account, "name" | "email" | "summary">[]>([]);
  const usage = useSyncExternalStore(app.settings.subscribe, () => app.settings.state.usage);
  const meters = usage.state === "ready" ? usage.data.meters : [];
  const [query, setQuery] = useState("");
  const anchorId = `run-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const providers = catalog.providers;

  // The meters are a cached read with no provider call; Settings may not have read them yet.
  useEffect(() => {
    // loadUsage keeps its own error in state.usage, so nothing waits for it.
    void app.settings.loadUsage();
  }, [app]);

  const accountInfo = providers.find((p) => p.id === accountCli(draft.provider, draft.model));
  useEffect(() => {
    let live = true;
    setAccounts([]);
    if (accountInfo?.accounts)
      app.life.runs.accounts(accountInfo.id).then(
        (a) => live && setAccounts(a.accounts),
        () => {}, // Accounts are optional; the default account starts.
      );
    return () => {
      live = false;
    };
  }, [app, accountInfo]);

  const { levels, defaultLevel, label } = runModel(catalog, draft);
  const q = query.trim().toLowerCase();
  // A failed accounts read still shows the chat's own account, which Apply sends.
  const accountRows = draft.account && !accounts.some((a) => a.name === draft.account) ? [{ name: draft.account, email: null, summary: null }, ...accounts] : accounts;

  const choose = (provider: string, m: ModelInfo | string) => {
    const cli = providers.find((x) => x.id === provider) ?? { id: provider, efforts: [], default_effort: "" };
    setDraft(chooseModel(current, draft, cli, m));
  };

  return (
  <fieldset disabled={disabled} className="grid grid-cols-2 gap-3">
      <div className="grid min-w-0 gap-1" onKeyDown={(e) => modelPickerKey(e, query)}>
        <input className={FIELD} data-testid="run-query" placeholder="Search or type a model id" aria-label="Model" spellCheck={false} autoComplete="off" value={query} onChange={(e) => setQuery(e.target.value)} />
        <div data-testid="run-models" className="max-h-[260px] overflow-y-auto">
          {(catalog.providers.length ? catalog.providers : [{ id: current.provider }]).map((p) => {
            const ms = catalog.models[p.id];
            const shown = Array.isArray(ms) ? ms.filter((m) => !q || m.label.toLowerCase().includes(q) || m.id.toLowerCase().includes(q)) : [];
            // A typed model id still works when the list could not be read.
            const custom = q && ms !== undefined && !(Array.isArray(ms) && ms.some((m) => m.id.toLowerCase() === q)) ? query.trim() : null;
            return (
              <div key={p.id} role="group" aria-labelledby={`${anchorId}-${p.id}`} data-provider={p.id}>
                <div id={`${anchorId}-${p.id}`} className="flex gap-2 px-2 pt-2 pb-1 text-xs text-muted-foreground">
                  <span className="flex-1 font-medium">{cliName(p.id)}</span>
                  {note?.(p.id) && <span>{note(p.id)}</span>}
                </div>
                {ms === undefined && <p className="px-2 py-1 text-xs text-muted-foreground">Reading models…</p>}
                {Array.isArray(ms) && !ms.length && <p className="px-2 py-1 text-xs text-muted-foreground">No models listed. Type a model id.</p>}
                {typeof ms === "string" && (
                  <p className="line-clamp-2 px-2 py-1 text-xs break-words text-failed" title={ms}>
                    {ms}
                  </p>
                )}
                {shown.map((m) => {
                  const on = draft.provider === p.id && draft.model === m.id;
                  return (
                    <button key={m.id} data-model-id={m.id} type="button" aria-pressed={on} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-hover aria-pressed:bg-selected" onClick={() => choose(p.id, m)}>
                      <Radio on={on} />
                      <span className="min-w-0 truncate">{m.label}</span>
                      <span className="ml-auto max-w-[55%] truncate text-xs text-muted-foreground" title={m.note}>{[m.note, current.provider === p.id && current.model === m.id && "now"].filter(Boolean).join(" · ")}</span>
                    </button>
                  );
                })}
                {custom && (
                  <button type="button" aria-pressed={draft.provider === p.id && draft.model === custom} data-model-id={custom} data-testid={`run-custom-${p.id}`} className="flex w-full min-w-0 rounded-md px-2 py-1 text-left hover:bg-hover aria-pressed:bg-selected" onClick={() => choose(p.id, custom)}>
                    <span className="truncate">Use “{custom}”</span>
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>
      <div className="grid content-start gap-3">
        <div className="grid gap-1.5" role="group" aria-label="Thinking" data-testid="run-thinking">
          <div className="flex gap-2 text-xs text-muted-foreground">
            <span className="flex-1 font-medium">Thinking</span>
            {levels.length > 0 && label && <span className="truncate">levels {label} allows</span>}
          </div>
          {levels.length ? (
            <div className="flex flex-wrap gap-1">
              {levels.map((l) => (
                <button
                  key={l}
                  type="button"
                  aria-pressed={draft.effort === l}
                  className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs hover:bg-hover aria-pressed:border-foreground aria-pressed:bg-foreground aria-pressed:text-background"
                  onClick={() => setDraft({ ...draft, effort: l })}
                >
                  {levelWord(l)}
                  {l === defaultLevel && <span className="text-[10px] opacity-70">default</span>}
                </button>
              ))}
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">This model sets its own thinking.</p>
          )}
        </div>
        <div className="grid gap-1" role="group" aria-label="Account" data-testid="run-account">
          <div className="flex gap-2 text-xs text-muted-foreground">
            <span className="flex-1 font-medium">Account</span>
            {accountInfo?.accounts && <span>limit left</span>}
          </div>
          {accountInfo?.accounts ? (
            [{ name: null, email: null, summary: null }, ...accountRows].map((a) => {
              const on = draft.account === a.name;
              const limit = a.name ? limitText(meters, accountInfo.id, a.name, a.summary ?? null) : null;
              return (
                <button key={a.name ?? ""} type="button" aria-pressed={on} title={[a.name && (a.email ? `${a.name} (${a.email})` : a.name), limit?.text].filter(Boolean).join("\n") || undefined} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-hover aria-pressed:bg-selected" onClick={() => setDraft({ ...draft, account: a.name })}>
                  <Radio on={on} />
                  {a.name && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(a.name) }} />}
                  <span className="min-w-0 truncate">{a.name ?? (draft.provider === "codex" && current.provider === "codex" && current.account ? `Keep current account (${current.account})` : "CLI default")}</span>
                  {limit?.text && (
                    <span className="ml-auto flex min-w-0 max-w-[60%] items-center gap-1.5 text-xs text-muted-foreground tabular-nums">
                      {limit.left !== null && (
                        <span aria-hidden className="h-1 w-6 shrink-0 overflow-hidden rounded-full bg-border">
                          <span className={`block h-full ${meterTone(limit.left)}`} style={{ width: `${limit.left}%` }} />
                        </span>
                      )}
                      <span className="truncate">{limit.text}</span>
                    </span>
                  )}
                </button>
              );
            })
          ) : (
            <span className="px-2 text-sm">{cliName(draft.provider)} login</span>
          )}
          {accountInfo?.accounts && <div className="text-xs text-muted-foreground">
            <span>{meters.filter((m) => m.provider === accountInfo.id).map((m) => m.as_of_seconds).filter((t): t is number => t !== null).sort((a, b) => a - b).slice(0, 1).map((t) => `Limits checked ${new Date(t * 1000).toLocaleString()}`).join("") || "No cached limit time"}</span>
            <Button type="button" variant="ghost" size="sm" onClick={() => void app.settings.refreshUsage(accountInfo.id)}>Refresh limits</Button>
          </div>}
        </div>
      </div>
    </fieldset>
  );
}

/** A radio mark for a pick row; the row's aria-pressed carries the state. */
function Radio({ on }: { on: boolean }) {
  return <span aria-hidden className={`size-3 shrink-0 rounded-full border ${on ? "border-[4px] border-foreground" : "border-border-strong"}`} />;
}
