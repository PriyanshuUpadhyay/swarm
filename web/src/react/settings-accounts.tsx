// Account settings and the usage HUD control.

import { useState } from "react";
import type { App } from "../core/app";
import { USAGE_PROVIDERS, type AccountList } from "../core/settings";
import { Button } from "../components/ui/button";
import { FIELD, label, Row, Toggle, useSettings, useNow } from "./settings-shared";
import { MeterLine, useWait } from "./usage-meters";

// ---- accounts ----

const AUTH: Record<string, string> = { signed_in: "Signed in", signed_out: "Signed out", unavailable: "Sign-in unknown" };

function AccountsPage({ app }: { app: App }) {
  const busy = useSettings(app, "busy");
  return (
    <div className="grid gap-6">
      <HudRow app={app} />
      {USAGE_PROVIDERS.map((p) => (
        <ProviderAccounts key={p} app={app} provider={p} />
      ))}
      <div>
        <Button size="sm" variant="outline" disabled={busy} data-testid="accounts-reset" onClick={() => app.settings.resetAccounts()}>
          Reset accounts…
        </Button>
      </div>
    </div>
  );
}

function HudRow({ app }: { app: App }) {
  const hud = useSettings(app, "hud");
  const pending = useSettings(app, "hudPending");
  const data = hud.state === "ready" ? hud.data : null;
  const title = "Usage HUD in the notch";
  const detail = pending
    ? "Working…"
    : hud.state === "error"
      ? `Usage HUD unavailable: ${hud.error}. Reopen Settings › Accounts to try again.`
      : data && data.app === null && !data.installed
        ? "SwarmHUD.app is not installed with this swarm."
        : "Shows usage at the top of the screen and starts at login.";
  return (
    <Row title={title} detail={detail} htmlFor="hud-switch" live>
      {/* Fire-and-forget is safe: setHud catches its own errors and posts a notice. */}
      <Toggle id="hud-switch" label={title} testid="hud-switch" checked={data?.installed ?? false} disabled={!data || (!data.installed && data.app === null)} busy={pending} onChange={(on) => void app.settings.setHud(on)} />
    </Row>
  );
}

function ProviderAccounts({ app, provider }: { app: App; provider: string }) {
  const list = useSettings(app, "accounts")[provider];
  const usage = useSettings(app, "usage");
  const busy = useSettings(app, "busy");
  const nextRefresh = useSettings(app, "nextRefresh");
  const now = useNow(Object.values(nextRefresh).some((t) => t > Date.now()) ? 1000 : 60_000);
  const wait = useWait(app, provider, now);
  const [name, setName] = useState("");
  const meters = usage.state === "ready" ? usage.data.meters.filter((m) => m.provider === provider) : [];
  const data: AccountList | null = list?.state === "ready" ? list.data : null;
  return (
    <section className="grid gap-2" aria-label={`${label(provider)} accounts`} data-testid={`accounts-${provider}`}>
      <div className="flex items-center gap-2">
        <h3 className="flex-1 text-sm font-semibold">{label(provider)}</h3>
        <Button size="sm" variant="ghost" data-testid="accounts-refresh" onClick={() => void Promise.all([app.settings.loadAccounts(provider), app.settings.loadUsage()])}>
          Refresh accounts
        </Button>
        <Button size="sm" variant="ghost" data-testid={`accounts-usage-refresh-${provider}`} disabled={wait !== null} title={wait ? `Next refresh in ${wait}` : undefined} onClick={() => void app.settings.refreshUsage(provider)}>
          {wait ? `Refresh usage in ${wait}` : "Refresh usage"}
        </Button>
      </div>
      {(!list || list.state === "loading" || list.state === "idle") && <p className="text-sm text-muted-foreground">Reading accounts…</p>}
      {list?.state === "error" && <p className="text-sm text-failed select-text">Accounts unavailable: {list.error}</p>}
      {data && data.state !== "ready" && <p className="text-sm text-muted-foreground">{data.state === "no_source" ? "This provider has no account source." : "Accounts are unavailable."}</p>}
      {data && (
        <ul className="grid gap-1">
          {data.accounts.map((a) => (
            <li key={a.name} className="grid gap-1 rounded-md border border-border px-3 py-2" data-testid="account-row" data-name={a.name}>
              <div className="flex items-center gap-2 text-sm">
                <span className="font-mono">{a.name}</span>
                {data.auto === a.name && <span className="rounded bg-selected px-1 text-xs text-muted-foreground">Auto</span>}
                <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{a.email ?? ""}</span>
                <span className="text-xs text-muted-foreground">{AUTH[a.auth_state] ?? a.auth_state}</span>
                {a.auth_state !== "signed_in" && a.name !== "default" && <Button size="sm" variant="outline" disabled={busy} onClick={() => void app.settings.login(provider, a.name)}>Sign in…</Button>}
              </div>
              {a.auth_state !== "signed_in" && a.name === "default" && <p className="text-xs text-muted-foreground">Sign in from a terminal with <code>{provider === "claude" ? "claude /login" : "codex login"}</code>.</p>}
              <div className="grid gap-0.5 text-xs">
                {meters
                  .filter((m) => m.account === a.name)
                  .map((m, i) => (
                    <MeterLine key={`${m.window}:${i}`} meter={m} now={now} />
                  ))}
              </div>
            </li>
          ))}
        </ul>
      )}
      <form
        data-settings-inline
        className="flex items-end gap-2"
        onKeyDown={(e) => { if (e.key === "Escape") { e.stopPropagation(); setName(""); } }}
        onSubmit={(e) => {
          e.preventDefault();
          void app.settings.login(provider, name.trim()).then((ok) => ok && setName(""));
        }}
      >
        <label className="grid min-w-0 flex-1 gap-1 text-xs text-muted-foreground">
          Add an account
          <input className={`${FIELD} font-mono`} data-testid="account-name" value={name} disabled={busy || !data} onChange={(e) => setName(e.target.value.toLowerCase())} placeholder="work" />
        </label>
        <Button size="sm" type="submit" data-testid="account-login" disabled={busy || !data || !name.trim()}>
          Open login
        </Button>
      </form>
    </section>
  );
}

export { AccountsPage };
