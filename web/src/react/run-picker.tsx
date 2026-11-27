// The run picker: models of each CLI, the thinking level and the account. The chat's run chip
// and the new chat composer both show it (ADR 0110, ADR 0149).

import { ArrowUpRightIcon, BrainIcon, SearchIcon, StarIcon } from "lucide-react";
import { useEffect, useState, useSyncExternalStore, type Dispatch, type ReactNode } from "react";
import type { App } from "../core/app";
import { accountCli, accountColor, chooseModel, cliName, levelWord, limitText, modelLevels, type Run } from "../core/composer";
import type { ModelInfo } from "../core/run-catalog";
import type { Account, ProviderInfo } from "../core/settings";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { message } from "./lifecycle-shared";
import { modelPickerKey } from "./model-picker";
import { meterTone } from "./usage-meters";
import { CliMark } from "./cli-mark";

const RAIL = "flex min-w-0 items-center gap-1.5 rounded-md px-2 py-1 text-left text-muted-foreground hover:bg-menu-hover aria-pressed:bg-menu-selected aria-pressed:text-foreground";

const ROW_ACTION = "opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [@media(hover:none)]:opacity-100 [@media(pointer:coarse)]:opacity-100";
/** A one-of-N pill that shows its pick with `aria-pressed`. */
export const PRESSED_PILL = "rounded-md border border-menu-border px-2 py-1 text-xs hover:bg-menu-hover aria-pressed:bg-menu-selected aria-pressed:text-foreground";

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
        setError(null);
        for (const p of providers) void read(p.id);
      },
      (err) => {
        if (!live) return;
        // The chat's own CLI still lists its models, and a typed id still works.
        setError(`Could not read the CLIs: ${message(err)}. Type a model id for this CLI.`);
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

/** One picker for a live run, an ended run, or a new chat draft. */
export function RunPicker({ app, catalog, current, draft, setDraft, disabled = false, preview, foot }: { app: App; catalog: Catalog; current?: Run; draft: Run; setDraft: Dispatch<Run>; disabled?: boolean; preview?: (run: Run | null) => void; foot: ReactNode }) {
  const [accounts, setAccounts] = useState<Pick<Account, "name" | "email" | "summary">[]>([]);
  const usage = useSyncExternalStore(app.settings.subscribe, () => app.settings.state.usage);
  const prefs = useSyncExternalStore(app.subscribe, () => app.view.tree?.prefs);
  const meters = usage.state === "ready" ? usage.data.meters : [];
  const [query, setQuery] = useState("");
  const [rail, setRail] = useState(current?.provider ?? draft.provider);
  const providers = catalog.providers.length ? catalog.providers : [{ id: current?.provider ?? draft.provider }];
  const favorites = prefs?.favoriteModels ?? [];

  // loadUsage keeps its errors in the usage state.
  useEffect(() => { void app.settings.loadUsage(); }, [app]);

  const accountInfo = catalog.providers.find((p) => p.id === accountCli(draft.provider, draft.model));
  useEffect(() => {
    let live = true;
    setAccounts([]);
    if (accountInfo?.accounts)
      app.life.runs.accounts(accountInfo.id).then(
        (a) => live && setAccounts(a.accounts),
        () => {}, // Accounts are optional; the default account starts.
      );
    return () => { live = false; };
  }, [app, accountInfo]);

  const { levels, defaultLevel } = runModel(catalog, draft);
  const q = query.trim().toLowerCase();
  const accountRows = draft.account && !accounts.some((a) => a.name === draft.account) ? [{ name: draft.account, email: null, summary: null }, ...accounts] : accounts;
  const list = catalog.models[rail];
  const shown = Array.isArray(list) ? list : [];
  const modelStatus = list === undefined ? "Reading models…" : typeof list === "string" ? list
    : shown.length ? `${shown.length} ${shown.length === 1 ? "model" : "models"} listed.` : "No models listed. Type a model id.";
  const selected = current ?? draft;
  const unlisted = rail === selected.provider && Array.isArray(list) && selected.model && !shown.some((m) => m.id === selected.model) ? selected.model : null;
  const custom = q && rail !== "favorites" && list !== undefined && (current || unlisted?.toLowerCase() !== q) && !shown.some((m) => m.id.toLowerCase() === q) ? query.trim() : null;

  const modelRun = (provider: string, m: ModelInfo | string) => {
    const cli = catalog.providers.find((p) => p.id === provider);
    return chooseModel(current ?? draft, draft, { id: provider, efforts: cli?.efforts ?? [], default_effort: cli?.default_effort ?? "" }, m);
  };
  const choose = (provider: string, m: ModelInfo | string) => setDraft(modelRun(provider, m));

  const modelRow = (provider: string, m: ModelInfo, nameCli: boolean, availability: "listed" | "unlisted" | "unavailable" = "listed") => {
    const on = draft.provider === provider && draft.model === m.id;
    const pair = { provider, model: m.id };
    const starred = favorites.some((f) => f.provider === provider && f.model === m.id);
    const missing = availability === "unavailable";

    if (q && ![m.label, m.id].some((text) => text.toLowerCase().includes(q))) return null;
    return (
      <div key={`${provider}/${m.id}`} data-testid={missing ? "run-unavailable" : undefined} className={`group flex items-center gap-1 rounded-md px-1 ${on ? "bg-menu-selected text-foreground" : "hover:bg-menu-hover"}`}>
        <button type="button" data-model-id={missing ? undefined : m.id} aria-pressed={on} disabled={missing} className="flex min-w-0 flex-1 items-center gap-2 rounded-md px-1 py-1.5 text-left" onClick={() => choose(provider, m)}
          onMouseEnter={() => preview?.(modelRun(provider, m))} onFocus={() => preview?.(modelRun(provider, m))}
          onMouseLeave={() => preview?.(null)} onBlur={() => preview?.(null)}>
          {nameCli && <><CliMark provider={provider} selected={on} /><span className="shrink-0 text-xs text-muted-foreground">{cliName(provider)}</span></>}
          <span className="min-w-0 truncate font-mono">{m.label}</span>
          {m.note && <span className="truncate text-xs text-muted-foreground" title={m.note}>{m.note}</span>}
          {missing && <span className="text-xs text-muted-foreground">Unavailable</span>}
          {current?.provider === provider && current.model === m.id && <span className="ml-auto rounded border border-menu-border px-1 text-xs text-muted-foreground">now</span>}
        </button>

        {!missing && <>
          {/* Choice writes report failures as app notices. */}
          <button type="button" title={starred ? "Remove from favorites" : "Add to favorites"} aria-label={`Favorites: ${m.label}`} aria-pressed={starred} className={`rounded p-0.5 hover:bg-menu-hover ${starred ? "" : ROW_ACTION}`} onClick={() => void app.settings.toggleFavorite(pair)}>
            <StarIcon className="size-3.5" fill={starred ? "currentColor" : "none"} />
          </button>
          {/* The preference writer reports a refusal as an app notice. */}
          {availability === "listed" && <button type="button" className={`rounded px-1 text-xs text-muted-foreground hover:bg-menu-hover ${ROW_ACTION}`} onClick={() => void app.settings.setPref("newChatModel", pair)}>
            {prefs?.newChatModel?.provider === provider && prefs.newChatModel.model === m.id ? "New chat default" : "Use for new chats"}
          </button>}
        </>}
      </div>
    );
  };
  const savedRow = ({ provider, model }: { provider: string; model: string }) => {
    const models = catalog.models[provider];
    const listed = Array.isArray(models) ? models.find((m) => m.id === model) : undefined;
    const availability = provider === "pi" && /\s/.test(model) ? "unavailable" : listed ? "listed" : "unlisted";
    return modelRow(provider, listed ?? { id: model, label: model }, provider !== current?.provider, availability);
  };

  return (
    <div className="grid min-w-0 grid-cols-[min(136px,30%)_minmax(0,1fr)] overflow-hidden rounded-[inherit]">
      <fieldset disabled={disabled} className="row-span-2 grid min-w-0 content-start gap-1 bg-surface p-1.5" aria-label="Model groups">
        <button type="button" aria-pressed={rail === "favorites"} className={RAIL} onClick={() => setRail("favorites")}><StarIcon className="size-4 shrink-0" /> Favorites</button>
        {providers.map((p) => <button key={p.id} type="button" aria-pressed={rail === p.id} className={RAIL} onClick={() => setRail(p.id)}>
          <CliMark provider={p.id} selected={rail === p.id} />
          <span className="truncate">{cliName(p.id)}</span>{current && p.id !== current.provider && <ArrowUpRightIcon className="size-3 shrink-0" aria-hidden />}
        </button>)}
      </fieldset>
      <fieldset disabled={disabled} className="min-w-0">
        <div className="grid min-w-0 gap-1 p-2" onKeyDown={(e) => modelPickerKey(e, query)}>
          <div className="relative">
            <SearchIcon aria-hidden className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input className={`w-full pl-7 [@media(pointer:coarse)]:text-[16px] ${current ? "pr-14" : ""}`} data-testid="run-query" placeholder="Search or type a model id" aria-label="Model" spellCheck={false} autoComplete="off" value={query} onChange={(e) => setQuery(e.target.value)} />
            {current && <kbd aria-hidden className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 rounded-sm border border-border px-1 font-mono text-xs text-muted-foreground">⇧⌘M</kbd>}
          </div>

          <div className="flex h-5 min-w-0 items-center gap-1 px-1 text-xs text-muted-foreground">
            {prefs?.newChatModel && <>
              <span className="min-w-0 flex-1 truncate">Default: {cliName(prefs.newChatModel.provider)} · <span className="font-mono">{prefs.newChatModel.model}</span></span>
              {/* The preference writer reports a refusal as an app notice. */}
              <button type="button" className="shrink-0 rounded px-1 hover:bg-menu-hover" aria-label="Reset new chat default" onClick={() => void app.settings.setPref("newChatModel", null)}>Reset</button>
            </>}
          </div>

          <span aria-live="polite" className="sr-only" data-testid="run-model-status">{rail === "favorites" ? "" : modelStatus}</span>
          <div data-testid="run-models" className="h-[min(300px,38vh)] overflow-y-auto">
            {rail !== "favorites" && !!prefs?.recentModels?.length && <div data-testid="run-recent">
              <p className="px-2 pt-1 text-xs text-muted-foreground">Recent</p>
              {prefs.recentModels.slice(0, 4).map(savedRow)}
            </div>}
            <div data-provider={rail}>
              <p className="px-2 py-1 text-xs text-muted-foreground">{rail === "favorites" ? "Favorites" : cliName(rail)}</p>
              {rail === "favorites" ? favorites.map(savedRow) : shown.map((m) => modelRow(rail, m, false))}
              {unlisted && modelRow(rail, { id: unlisted, label: unlisted }, false, current ? "unavailable" : "unlisted")}
              {rail !== "favorites" && !shown.length && <p className={`line-clamp-2 px-2 text-xs ${typeof list === "string" ? "text-failed" : "text-muted-foreground"}`}>{modelStatus}</p>}
              {rail === "favorites" && !favorites.length && <p className="px-2 text-xs text-muted-foreground">Star a model to see it here.</p>}
              {custom && <button type="button" data-model-id={custom} data-testid={`run-custom-${rail}`} className="w-full truncate rounded-md px-2 py-1 text-left hover:bg-menu-hover" onClick={() => choose(rail, custom)}>Use “<span className="font-mono">{custom}</span>”</button>}
            </div>
          </div>
        </div>
        <div className="grid gap-2 border-t border-menu-border p-2">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1" role="group" aria-label="Thinking" data-testid="run-thinking">
          <span className="flex items-center gap-1.5 text-sm/4 text-muted-foreground">
            <BrainIcon aria-hidden className="size-3.5" /> Thinking
          </span>
          {levels.length ? (
            <div className="flex flex-wrap gap-1">
              {levels.map((l) => (
                <button
                  key={l}
                  type="button"
                  aria-pressed={draft.effort === l}
                  className={`flex items-center gap-1 ${PRESSED_PILL}`}
                  onClick={() => setDraft({ ...draft, effort: l })}
                >
                  {levelWord(l)}
                  {l === defaultLevel && <span className="text-xs opacity-70">default</span>}
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
                <button key={a.name ?? ""} type="button" aria-pressed={on} title={[a.name && (a.email ? `${a.name} (${a.email})` : a.name), limit?.text].filter(Boolean).join("\n") || undefined} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-menu-hover aria-pressed:bg-menu-hover" onClick={() => setDraft({ ...draft, account: a.name })}>
                  <Radio on={on} />
                  {a.name && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(a.name) }} />}
                  <span className="min-w-0 truncate">{a.name ?? (draft.provider === "codex" && current?.provider === "codex" && current.account ? `Keep current account (${current.account})` : "CLI default")}</span>
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
      <div className="col-start-2 border-t border-menu-border text-sm/4 text-muted-foreground">{foot}</div>
    </div>
  );
}

/** A radio mark for a pick row; the row's aria-pressed carries the state. */
function Radio({ on }: { on: boolean }) {
  return <span aria-hidden className={`size-3 shrink-0 rounded-full border ${on ? "border-[4px] border-foreground" : "border-border"}`} />;
}
