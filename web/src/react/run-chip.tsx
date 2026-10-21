// The run chip `account · model · thinking` and its popover, which changes the chat's run
// (ADR 0108). Inside one CLI the session restarts on the new run; another CLI asks what to bring.

import { CheckIcon, ChevronDownIcon, LoaderCircleIcon } from "lucide-react";
import { useEffect, useId, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { App } from "../core/app";
import { outcomeText, runLabel, switchOutcome, type AgentInput, type Carry, type Run } from "../core/composer";
import type { ProviderInfo } from "../core/settings";
import { Button } from "../components/ui/button";
import { FIELD, message, useLife, ErrorLine } from "./lifecycle-shared";
import { SwitchProgress } from "./lifecycle-switch-model";
import { Segmented } from "./settings-shared";

type Model = { id: string; label: string; efforts?: string[] };

export function RunChip({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const chosen = useLife(app, "switchModel");
  const handoff = useLife(app, "handoff");
  const open = chosen === sessionId && !chair.who && !chair.unreachable;
  const pop = useRef<HTMLDivElement>(null);
  // An anchor name must be a CSS ident; useId gives characters outside one.
  const anchor = `--run-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const restarting = handoff?.old === sessionId && !["completed", "failed", "cancelled"].includes(handoff.phase);

  useEffect(() => {
    const el = pop.current;
    if (!el) return;
    if (open && !el.matches(":popover-open")) el.showPopover();
    if (!open && el.matches(":popover-open")) el.hidePopover();
  }, [open]);

  return (
    <>
      <button
        type="button"
        data-testid="model-chip"
        className="flex min-w-0 items-center gap-1 rounded-md px-1.5 py-1 hover:bg-hover disabled:pointer-events-none"
        title={chair.who ? "Change the child from its parent agent." : "Change model, thinking, or account"}
        disabled={chair.who !== null}
        aria-expanded={open}
        style={{ anchorName: anchor } as CSSProperties}
        onClick={() => (open ? app.life.close("switchModel") : app.life.openSwitchModel(sessionId))}
      >
        <span className="truncate">{restarting ? "Restarting…" : runLabel(chair)}</span>
        {!chair.who && <ChevronDownIcon className="size-3 shrink-0" />}
      </button>
      {!chair.who && (
        <div
          ref={pop}
          popover="auto"
          data-testid="run-popover"
          aria-label="Run"
          className="m-0 w-[min(640px,calc(100vw-32px))] rounded-xl border border-border-strong bg-popover p-3 text-sm text-popover-foreground shadow-panel"
          style={{ positionAnchor: anchor, positionArea: "top span-right", marginBottom: 6, positionTryFallbacks: "flip-block" } as CSSProperties}
          onToggle={(e) => e.newState === "closed" && app.life.state.switchModel === sessionId && app.life.close("switchModel")}
        >
          {open && <RunBody app={app} sessionId={sessionId} chair={chair} key={sessionId} />}
        </div>
      )}
    </>
  );
}

function RunBody({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const handoff = useLife(app, "handoff");
  const saved: Carry = useSyncExternalStore(app.subscribe, () => (app.view.tree?.prefs?.switchCarry === "clean" ? "clean" : "summary"));
  const current: Run = { provider: chair.provider ?? "claude", model: chair.model, effort: chair.effort, account: chair.account };
  const [draft, setDraft] = useState<Run>(current);
  const [carry, setCarry] = useState<Carry>(saved);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [models, setModels] = useState<Record<string, Model[] | string>>({});
  const [accounts, setAccounts] = useState<{ name: string; email: string | null }[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const running = handoff?.old === sessionId && !["completed", "failed", "cancelled"].includes(handoff.phase);

  useEffect(() => {
    let live = true;
    app.life.providers().then(
      ({ providers }) => {
        if (!live) return;
        setProviders(providers);
        for (const p of providers)
          app.life.models(p.id).then(
            (m) => live && setModels((all) => ({ ...all, [p.id]: m.models })),
            (err) => live && setModels((all) => ({ ...all, [p.id]: `Could not read the models: ${message(err)}` })),
          );
      },
      (err) => live && setError(`Could not read the providers: ${message(err)}`),
    );
    return () => {
      live = false;
    };
  }, [app]);

  const info = providers.find((p) => p.id === draft.provider);
  useEffect(() => {
    let live = true;
    setAccounts([]);
    if (info?.accounts)
      app.life.accounts(info.id).then(
        (a) => live && setAccounts(a.accounts),
        () => {}, // Accounts are optional; the default account starts.
      );
    return () => {
      live = false;
    };
  }, [app, info]);

  const list = models[draft.provider];
  const model = Array.isArray(list) ? list.find((m) => m.id === draft.model) : undefined;
  const levels = model?.efforts ?? info?.efforts ?? [];
  const outcome = switchOutcome(current, draft);
  const q = query.trim().toLowerCase();
  const blocked = chair.running ? "Wait for this turn to end, or stop it." : !draft.model ? "Pick a model." : null;

  const choose = (provider: string, m: Model | string) => {
    const id = typeof m === "string" ? m : m.id;
    const p = providers.find((x) => x.id === provider);
    const allowed = (typeof m === "string" ? null : m.efforts) ?? p?.efforts ?? [];
    const effort = draft.effort && allowed.includes(draft.effort) ? draft.effort : allowed.length ? (p?.default_effort ?? allowed[0]) : null;
    setDraft({ provider, model: id, effort, account: provider === draft.provider ? draft.account : null });
  };

  const apply = () => {
    if (!draft.model || outcome === "none") return;
    void app.life.switchModel(sessionId, draft.provider, draft.model, draft.account, draft.effort, outcome === "resume" ? "resume" : carry);
  };

  return (
    <div className="grid gap-3">
      <fieldset disabled={running} className="grid grid-cols-[1fr_200px] gap-3">
        <div className="grid min-w-0 gap-1">
          <input className={FIELD} data-testid="run-query" placeholder="Search or type a model id" aria-label="Model" value={query} onChange={(e) => setQuery(e.target.value)} />
          <div role="listbox" aria-label="Models" data-testid="run-models" className="max-h-[260px] overflow-y-auto">
            {(providers.length ? providers : [{ id: current.provider, label: current.provider } as ProviderInfo]).map((p) => {
              const ms = models[p.id];
              const shown = Array.isArray(ms) ? ms.filter((m) => !q || m.label.toLowerCase().includes(q) || m.id.toLowerCase().includes(q)) : [];
              // A typed model id still works when the list could not be read.
              const custom = q && ms !== undefined && !(Array.isArray(ms) && ms.some((m) => m.id.toLowerCase() === q)) ? query.trim() : null;
              return (
                <div key={p.id} role="group" aria-label={p.label} data-provider={p.id}>
                  <div className="px-2 pt-2 pb-1 text-xs text-muted-foreground">{p.label}</div>
                  {ms === undefined && <p className="px-2 py-1 text-xs text-muted-foreground">Reading models…</p>}
                  {typeof ms === "string" && (
                    <p className="line-clamp-2 px-2 py-1 text-xs break-words text-failed" title={ms}>
                      {ms}
                    </p>
                  )}
                  {shown.map((m) => {
                    const on = draft.provider === p.id && draft.model === m.id;
                    return (
                      <button key={m.id} type="button" role="option" aria-selected={on} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-hover aria-selected:bg-selected" onClick={() => choose(p.id, m)}>
                        <span className="min-w-0 flex-1 truncate">{m.label}</span>
                        {current.provider === p.id && current.model === m.id && <span className="text-xs text-muted-foreground">now</span>}
                        {on && <CheckIcon className="size-3.5" />}
                      </button>
                    );
                  })}
                  {custom && (
                    <button type="button" role="option" aria-selected={draft.provider === p.id && draft.model === custom} data-testid={`run-custom-${p.id}`} className="flex w-full rounded-md px-2 py-1 text-left hover:bg-hover aria-selected:bg-selected" onClick={() => choose(p.id, custom)}>
                      Use “{custom}”
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
        <div className="grid content-start gap-3">
          <div className="grid gap-1" role="radiogroup" aria-label="Thinking" data-testid="run-thinking">
            <div className="text-xs text-muted-foreground">Thinking</div>
            {levels.length ? (
              levels.map((l) => (
                <button key={l} type="button" role="radio" aria-checked={draft.effort === l} className="flex items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-hover aria-checked:bg-selected" onClick={() => setDraft({ ...draft, effort: l })}>
                  <span className="flex-1">{l}</span>
                  {draft.effort === l && <CheckIcon className="size-3.5" />}
                </button>
              ))
            ) : (
              <p className="text-xs text-muted-foreground">This model sets its own thinking.</p>
            )}
          </div>
          <label className="grid gap-1 text-xs text-muted-foreground">
            Account
            {info?.accounts ? (
              <select className={FIELD} data-testid="run-account" value={draft.account ?? ""} onChange={(e) => setDraft({ ...draft, account: e.target.value || null })}>
                <option value="">Default</option>
                {accounts.map((a) => (
                  <option key={a.name} value={a.name}>
                    {a.email ? `${a.name} (${a.email})` : a.name}
                  </option>
                ))}
              </select>
            ) : (
              <span className="text-sm text-foreground">{info?.label ?? draft.provider} login</span>
            )}
          </label>
        </div>
      </fieldset>
      {outcome === "handoff" && !running && (
        <div className="flex items-center gap-2" data-testid="run-carry">
          <span className="text-xs text-muted-foreground">Bring over</span>
          <Segmented<Carry>
            label="Bring over"
            testid="run-carry-choice"
            value={carry}
            options={[
              ["summary", "Summary"],
              ["clean", "Start clean"],
            ]}
            onChange={setCarry}
          />
          {carry !== saved && (
            <Button type="button" variant="ghost" size="sm" data-testid="run-carry-default" onClick={() => void app.settings.setPref("switchCarry", carry)}>
              Make default
            </Button>
          )}
        </div>
      )}
      {handoff?.old === sessionId && <SwitchProgress handoff={handoff} />}
      <ErrorLine text={(handoff?.old === sessionId ? handoff.error : null) ?? error} />
      <div className="flex items-center gap-2">
        <p className="min-w-0 flex-1 text-xs text-muted-foreground" data-testid="run-outcome" role="status">
          {running ? "Switching…" : (blocked ?? outcomeText(outcome, draft, carry))}
        </p>
        {running ? (
          <Button type="button" variant="outline" size="sm" data-testid="run-cancel" disabled={!handoff.canCancel} onClick={() => void app.life.cancelSwitch()}>
            Cancel switch
          </Button>
        ) : (
          <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("switchModel")}>
            Close
          </Button>
        )}
        <Button type="button" size="sm" data-testid="run-apply" disabled={running || blocked !== null || outcome === "none"} onClick={apply}>
          {running && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
          Apply
        </Button>
      </div>
    </div>
  );
}
