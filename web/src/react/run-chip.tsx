// The run chip `account · model · thinking` and its popover, which changes the chat's run
// (ADR 0110). Inside one CLI the session restarts on the new run; another CLI asks what to bring.

import { ChevronDownIcon, LoaderCircleIcon, XIcon } from "lucide-react";
import { useEffect, useId, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { App } from "../core/app";
import { accountColor, applyBlock, waitsForTurn, applyLabel, CARRY_OPTIONS, chooseModel, cliName, levelWord, limitText, modelLevels, outcomeText, runLabel, runTip, savedCarry, switchOutcome, type AgentInput, type Carry, type Run } from "../core/composer";
import { isSettled } from "../core/lifecycle";
import type { ModelInfo } from "../core/run-catalog";
import type { Account, ProviderInfo } from "../core/settings";
import { Button } from "../components/ui/button";
import { FIELD, message, useLife, ErrorLine } from "./lifecycle-shared";
import { SwitchProgress } from "./lifecycle-switch-model";
import { Segmented } from "./settings-shared";
import { meterTone } from "./usage-meters";

export function RunChip({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const chosen = useLife(app, "switchModel");
  const handoff = useLife(app, "handoffs")[sessionId];
  const open = chosen === sessionId && !chair.who && !chair.unreachable;
  const pop = useRef<HTMLDivElement>(null);
  // An anchor name must be a CSS ident; useId gives characters outside one.
  const anchor = `--run-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const switching = handoff && !isSettled(handoff.phase) ? (handoff.carry === "resume" ? "Restarting…" : "Switching…") : null;
  // The queue holds the chair's change; a child's chip shares the chat id but not that change.
  const queued = useSyncExternalStore(app.life.queue.subscribe, () => app.life.queue.state[sessionId]);
  const pending = chair.who ? undefined : queued;
  const run: Run = { provider: chair.provider ?? "claude", model: chair.model, effort: chair.effort, account: chair.account };

  useEffect(() => {
    const el = pop.current;
    if (!el) return;
    if (open && !el.matches(":popover-open")) {
      el.showPopover();
      // A popover takes focus only from an `autofocus` child at the moment it opens.
      el.querySelector<HTMLInputElement>("[data-testid=run-query]")?.focus();
    }
    if (!open && el.matches(":popover-open")) el.hidePopover();
  }, [open]);
  // A closed pane swaps the composer for Resume and Continue chat, which removes this chip with
  // no toggle event; the open state must not outlive it.
  // A child's chip shares the chat's id and never opens, so only the chair's chip closes it.
  const own = chair.who === null;
  useEffect(() => () => void (own && app.life.state.switchModel === sessionId && app.life.close("switchModel")), [app, sessionId, own]);

  const pendingText = pending ? `Pending: ${[pending.model, pending.effort && levelWord(pending.effort)].filter(Boolean).join(" · ")}` : "";

  return (
    <>
      <button
        type="button"
        data-testid="model-chip"
        className="flex min-w-0 items-center gap-1 rounded-md px-1.5 py-1 hover:bg-hover disabled:pointer-events-none"
        title={chair.who ? "Change the child from its parent agent." : runTip(run, chair.account && app.life.runs.email(run.provider, chair.account))}
        disabled={chair.who !== null}
        aria-expanded={open}
        style={{ anchorName: anchor } as CSSProperties}
        onClick={() => (open ? app.life.close("switchModel") : app.life.openSwitchModel(sessionId))}
      >
        {pending ? (
          <span aria-hidden className="size-2 shrink-0 rounded-full bg-waiting" />
        ) : (
          chair.account && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(chair.account) }} />
        )}
        <span className="truncate" data-testid="model-chip-label">
          {switching ?? (pending ? pendingText : runLabel(chair))}
        </span>
        {!chair.who && <ChevronDownIcon className="size-3 shrink-0" />}
      </button>
      {/* A button's children are presentational, so a queued change and a running switch are announced from outside it. */}
      <span role="status" className="sr-only" data-testid="run-chip-status">
        {switching ?? (pending ? pendingText : "")}
      </span>
      {pending && (
        <button type="button" className="icon-btn" aria-label="Cancel change" title="Cancel change" data-testid="run-pending-cancel" onClick={() => app.life.queue.cancel(sessionId)}>
          <XIcon className="size-3" />
        </button>
      )}
      {!chair.who && (
        <div
          ref={pop}
          popover="auto"
          // Only an open dialog has the role; ⌘K waits for no dialog before it focuses the composer.
          role={open ? "dialog" : undefined}
          data-testid="run-popover"
          aria-label="Change model, thinking, or account"
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
  const handoff = useLife(app, "handoffs")[sessionId];
  const saved = useSyncExternalStore(app.subscribe, () => savedCarry(app.view.tree?.prefs));
  const current: Run = { provider: chair.provider ?? "claude", model: chair.model, effort: chair.effort, account: chair.account };
  const provider = current.provider;
  const [draft, setDraft] = useState<Run>(current);
  const [carry, setCarry] = useState<Carry>(saved);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [models, setModels] = useState<Record<string, ModelInfo[] | string>>({});
  const [accounts, setAccounts] = useState<Pick<Account, "name" | "email" | "summary">[]>([]);
  const usage = useSyncExternalStore(app.settings.subscribe, () => app.settings.state.usage);
  const meters = usage.state === "ready" ? usage.data.meters : [];
  const [query, setQuery] = useState("");
  const anchorId = `run-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const [error, setError] = useState<string | null>(null);
  const running = handoff !== undefined && !isSettled(handoff.phase);

  // The meters are a cached read with no provider call; Settings may not have read them yet.
  useEffect(() => {
    // loadUsage keeps its own error in state.usage, so nothing waits for it.
    if (app.settings.state.usage.state === "idle") void app.settings.loadUsage();
  }, [app]);

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

  const info = providers.find((p) => p.id === draft.provider);
  useEffect(() => {
    let live = true;
    setAccounts([]);
    if (info?.accounts)
      app.life.runs.accounts(info.id).then(
        (a) => live && setAccounts(a.accounts),
        () => {}, // Accounts are optional; the default account starts.
      );
    return () => {
      live = false;
    };
  }, [app, info]);

  const list = models[draft.provider];
  const model = Array.isArray(list) ? list.find((m) => m.id === draft.model) : undefined;
  const levels = modelLevels(info?.efforts ?? [], model?.efforts);
  const defaultLevel = model?.default_effort ?? info?.default_effort;
  const outcome = switchOutcome(current, draft);
  // Apply queues whenever the queue would wait, so a direct switch never leaves an older queued one behind.
  const working = waitsForTurn(chair, app.life.state.handoffs[sessionId]);
  const q = query.trim().toLowerCase();
  const blocked = applyBlock(chair, draft);
  // A failed accounts read still shows the chat's own account, which Apply sends.
  const accountRows = draft.account && !accounts.some((a) => a.name === draft.account) ? [{ name: draft.account, email: null, summary: null }, ...accounts] : accounts;
  const label = model?.label ?? draft.model ?? "";

  const choose = (provider: string, m: ModelInfo | string) => {
    const cli = providers.find((x) => x.id === provider) ?? { id: provider, efforts: [], default_effort: "" };
    setDraft(chooseModel(current, draft, cli, m));
  };

  const apply = () => {
    if (!draft.model || outcome === "none") return;
    const how = outcome === "resume" ? "resume" : carry;
    if (working) {
      app.life.queue.add(sessionId, { provider: draft.provider, model: draft.model, account: draft.account, effort: draft.effort, carry: how });
      app.life.close("switchModel");
    } else {
      // switchModel keeps its own failure in the handoff state, so nothing waits for it.
      void app.life.switchModel(sessionId, draft.provider, draft.model, draft.account, draft.effort, how);
    }
  };

  return (
    <div className="grid gap-3">
      <fieldset disabled={running} className="grid grid-cols-2 gap-3">
        <div className="grid min-w-0 gap-1">
          <input className={FIELD} data-testid="run-query" placeholder="Search or type a model id" aria-label="Model" spellCheck={false} autoComplete="off" value={query} onChange={(e) => setQuery(e.target.value)} />
          <div data-testid="run-models" className="max-h-[260px] overflow-y-auto">
            {(providers.length ? providers : [{ id: current.provider }]).map((p) => {
              const ms = models[p.id];
              const shown = Array.isArray(ms) ? ms.filter((m) => !q || m.label.toLowerCase().includes(q) || m.id.toLowerCase().includes(q)) : [];
              // A typed model id still works when the list could not be read.
              const custom = q && ms !== undefined && !(Array.isArray(ms) && ms.some((m) => m.id.toLowerCase() === q)) ? query.trim() : null;
              return (
                <div key={p.id} role="group" aria-labelledby={`${anchorId}-${p.id}`} data-provider={p.id}>
                  <div id={`${anchorId}-${p.id}`} className="flex gap-2 px-2 pt-2 pb-1 text-xs text-muted-foreground">
                    <span className="flex-1 font-medium">{cliName(p.id)}</span>
                    {p.id !== current.provider && <span>{carry === "summary" ? "new chat + summary" : "new chat"}</span>}
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
                      <button key={m.id} type="button" aria-pressed={on} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-hover aria-pressed:bg-selected" onClick={() => choose(p.id, m)}>
                        <Radio on={on} />
                        <span className="min-w-0 truncate">{m.label}</span>
                        <span className="ml-auto max-w-[55%] truncate text-xs text-muted-foreground" title={m.note}>{[m.note, current.provider === p.id && current.model === m.id && "now"].filter(Boolean).join(" · ")}</span>
                      </button>
                    );
                  })}
                  {custom && (
                    <button type="button" aria-pressed={draft.provider === p.id && draft.model === custom} data-testid={`run-custom-${p.id}`} className="flex w-full min-w-0 rounded-md px-2 py-1 text-left hover:bg-hover aria-pressed:bg-selected" onClick={() => choose(p.id, custom)}>
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
              {info?.accounts && <span>limit left</span>}
            </div>
            {info?.accounts ? (
              [{ name: null, email: null, summary: null }, ...accountRows].map((a) => {
                const on = draft.account === a.name;
                const limit = a.name ? limitText(meters, draft.provider, a.name, a.summary ?? null) : null;
                return (
                  <button key={a.name ?? ""} type="button" aria-pressed={on} title={a.name ? (a.email ? `${a.name} (${a.email})` : a.name) : undefined} className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-hover aria-pressed:bg-selected" onClick={() => setDraft({ ...draft, account: a.name })}>
                    <Radio on={on} />
                    {a.name && <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: accountColor(a.name) }} />}
                    <span className="min-w-0 truncate">{a.name ?? "CLI default"}</span>
                    {limit?.text && (
                      <span className="ml-auto flex min-w-0 max-w-[60%] items-center gap-1.5 text-xs text-muted-foreground tabular-nums" title={limit.text}>
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
          </div>
        </div>
      </fieldset>
      {outcome === "handoff" && !running && (
        <div className="flex flex-wrap items-center gap-2" data-testid="run-carry">
          <span className="text-xs text-muted-foreground">Bring over</span>
          <Segmented<Carry>
            label="Bring over"
            testid="run-carry-choice"
            value={carry}
            options={CARRY_OPTIONS}
            onChange={setCarry}
          />
          {/* setPref shows its own notice on failure, so the promise never rejects. */}
          {carry !== saved && (
            <Button type="button" variant="ghost" size="sm" data-testid="run-carry-default" onClick={() => void app.settings.setPref("switchCarry", carry)}>
              Make default
            </Button>
          )}
        </div>
      )}
      {handoff && <SwitchProgress handoff={handoff} />}
      <ErrorLine text={handoff?.error ?? error} />
      <div className={`rounded-lg border px-3 py-2 text-xs ${outcome === "none" && !running ? "border-dashed border-border text-muted-foreground" : "border-border bg-hover"}`} role="status">
        {outcome !== "none" && !running && !blocked && <b className="mr-1.5 font-medium text-foreground">What happens</b>}
        <span data-testid="run-outcome">{running ? "Switching…" : (blocked ?? outcomeText(outcome, current, draft, carry, { label, working }))}</span>
      </div>
      <div className="flex items-center justify-end gap-2">
        {running ? (
          <Button type="button" variant="outline" size="sm" data-testid="run-cancel" disabled={!handoff?.canCancel} onClick={() => void app.life.cancelSwitch(sessionId)}>
            Cancel switch
          </Button>
        ) : (
          <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("switchModel")}>
            Cancel
          </Button>
        )}
        <Button type="button" size="sm" data-testid="run-apply" disabled={running || blocked !== null || outcome === "none"} onClick={apply}>
          {running && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
          {applyLabel(outcome, working)}
        </Button>
      </div>
    </div>
  );
}

/** A radio mark for a pick row; the row's aria-pressed carries the state. */
function Radio({ on }: { on: boolean }) {
  return <span aria-hidden className={`size-3 shrink-0 rounded-full border ${on ? "border-[4px] border-foreground" : "border-border-strong"}`} />;
}
