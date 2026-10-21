// Model switching and handoff progress.

import { CheckIcon, CircleIcon, LoaderCircleIcon, XIcon } from "lucide-react";
import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { CARRY_OPTIONS, savedCarry, type Carry } from "../core/composer";
import { isSettled, type Handoff } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { FIELD, message, useLife, Frame, Label, ErrorLine } from "./lifecycle-shared";
import { Segmented } from "./settings-shared";

// ---- Switch model ----

const PHASES: [string, string][] = [
  ["preparing", "Read this chat"],
  ["summarizing", "Ask the agent for a summary"],
  ["starting", "Start the new chat"],
  ["waiting", "Wait for the new agent"],
  ["delivering", "Send the summary"],
];

function SwitchModelDialog({ app }: { app: App }) {
  const chosen = useLife(app, "switchModel");
  // A live chair switches in the run chip's popover; this dialog continues an ended chat.
  const unreachable = useSyncExternalStore(app.subscribe, () => chosen !== null && app.chair(chosen).unreachable);
  const old = chosen && unreachable ? chosen : null;
  return (
    <Frame open={old !== null} onClose={() => app.life.close("switchModel")} title="Switch model" description="A new chat on the chosen model continues this one." testid="switch-model">
      {old && <SwitchModelBody app={app} old={old} key={old} />}
    </Frame>
  );
}

function SwitchModelBody({ app, old }: { app: App; old: string }) {
  const handoff = useLife(app, "handoffs")[old] ?? null;
  const saved = useSyncExternalStore(app.subscribe, () => savedCarry(app.view.tree?.prefs));
  const [carry, setCarry] = useState<Carry>(saved);
  const [providers, setProviders] = useState<{ id: string; label: string; accounts: boolean }[]>([]);
  const [provider, setProvider] = useState("claude");
  const [models, setModels] = useState<{ id: string; label: string }[] | null>(null);
  const [accounts, setAccounts] = useState<{ name: string; email: string | null }[]>([]);
  const [account, setAccount] = useState("");
  const [query, setQuery] = useState("");
  const [model, setModel] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = handoff !== null && !isSettled(handoff.phase);

  useEffect(() => {
    app.life.runs.providers().then(
      (p) => setProviders(p.providers),
      (err) => setError(`Could not read the providers: ${message(err)}`),
    );
  }, [app]);
  const hasAccounts = providers.find((p) => p.id === provider)?.accounts ?? false;
  useEffect(() => {
    let live = true;
    setModels(null);
    setModel(null);
    setAccounts([]);
    setAccount("");
    setError(null);
    app.life.runs.models(provider).then(
      (m) => live && setModels(m.models),
      (err) => {
        if (!live) return;
        // A typed model id still works without the list.
        setModels([]);
        setError(`Could not read the models: ${message(err)}`);
      },
    );
    if (hasAccounts)
      app.life.runs.accounts(provider).then(
        (a) => live && setAccounts(a.accounts),
        () => {}, // Accounts are optional; the default account starts.
      );
    return () => {
      live = false;
    };
  }, [app, provider, hasAccounts]);

  const q = query.trim();
  const shown = useMemo(() => (models ?? []).filter((m) => !q || m.label.toLowerCase().includes(q.toLowerCase())), [models, q]);
  const custom = q && !(models ?? []).some((m) => m.id === q) ? q : null;

  return (
    <div className="grid gap-3">
      <fieldset disabled={running} className="grid gap-3">
        <div className="grid grid-cols-2 gap-2">
          <Label text="Provider">
            <select className={FIELD} data-testid="sm-provider" value={provider} onChange={(e) => setProvider(e.target.value)}>
              {(providers.length ? providers : [{ id: "claude", label: "Claude" }]).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </Label>
          <Label text="Account">
            <select className={FIELD} data-testid="sm-account" value={account} disabled={!hasAccounts} onChange={(e) => setAccount(e.target.value)}>
              <option value="">Default</option>
              {accounts.map((a) => (
                <option key={a.name} value={a.name}>
                  {a.email ? `${a.name} (${a.email})` : a.name}
                </option>
              ))}
            </select>
          </Label>
        </div>
        <Label text="Model">
          <input className={FIELD} data-testid="sm-query" placeholder="Search or type a model id" value={query} onChange={(e) => setQuery(e.target.value)} />
        </Label>
        <div data-testid="sm-models" className="max-h-[160px] overflow-y-auto rounded-md border border-border">
          {!models && <p className="px-2 py-1.5 text-sm text-muted-foreground">Reading models…</p>}
          {models?.length === 0 && !custom && <p className="px-2 py-1.5 text-sm text-muted-foreground">No models listed. Type a model id.</p>}
          {shown.map((m) => (
            <button key={m.id} type="button" aria-pressed={model === m.id} className="flex w-full items-center gap-2 px-2 py-1 text-left text-sm hover:bg-hover aria-pressed:bg-selected" onClick={() => setModel(m.id)}>
              <span className="min-w-0 flex-1 truncate">{m.label}</span>
              {model === m.id && <CheckIcon className="size-3.5" />}
            </button>
          ))}
          {custom && (
            <button type="button" aria-pressed={model === custom} data-testid="sm-custom" className="flex w-full items-center gap-2 px-2 py-1 text-left text-sm hover:bg-hover aria-pressed:bg-selected" onClick={() => setModel(custom)}>
              Use “{custom}”
            </button>
          )}
        </div>
        <div className="grid gap-1 text-xs text-muted-foreground">
          Bring over
          <Segmented<Carry>
            label="Bring over"
            testid="sm-carry"
            value={carry}
            options={CARRY_OPTIONS}
            onChange={setCarry}
          />
        </div>
      </fieldset>
      {handoff && <SwitchProgress handoff={handoff} />}
      <ErrorLine text={handoff?.error ?? error} />
      <div className="flex justify-end gap-2">
        {running ? (
          <Button type="button" variant="outline" size="sm" data-testid="sm-cancel" disabled={!handoff.canCancel} onClick={() => void app.life.cancelSwitch(old)}>
            Cancel switch
          </Button>
        ) : (
          <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("switchModel")}>
            Close
          </Button>
        )}
        <Button type="button" size="sm" data-testid="sm-start" disabled={!model || running} onClick={() => model && void app.life.switchModel(old, provider, model, account || null, null, carry)}>
          {running && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
          Switch
        </Button>
      </div>
    </div>
  );
}

/** The hand-off steps as they run, also in the run chip's popover. */
function SwitchProgress({ handoff }: { handoff: Handoff }) {
  const running = !isSettled(handoff.phase);
  // A resume and Start clean only stop the old run and start the new one.
  const steps = handoff.carry === "resume" || handoff.carry === "clean" ? PHASES.filter(([p]) => p === "preparing" || p === "starting") : PHASES;
  const phaseIndex = steps.findIndex(([p]) => p === handoff.phase);
  return (
    <ol className="grid gap-1 text-sm" data-testid="sm-progress" data-phase={handoff.phase} aria-label="Progress" aria-live="polite">
      {steps.map(([p, label], i) => {
        const now = handoff.phase === p;
        const done = phaseIndex > i || handoff.phase === "completed";
        return (
          <li key={p} className={`flex items-center gap-2 ${now ? "text-foreground" : "text-muted-foreground"}`}>
            {now && running ? <LoaderCircleIcon className="size-3.5 animate-spin motion-reduce:animate-none" /> : done ? <CheckIcon className="size-3.5 text-done" /> : <CircleIcon className="size-3.5" />}
            {label}
          </li>
        );
      })}
      {handoff.phase === "cancelled" && (
        <li className="flex items-center gap-2 text-muted-foreground">
          <XIcon className="size-3.5" /> Cancelled. This chat is unchanged.
        </li>
      )}
    </ol>
  );
}

export { SwitchModelDialog, SwitchProgress };
