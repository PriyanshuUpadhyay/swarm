// Settings (⌘,) with five pages, and the compact usage meters in the sidebar footer (S5). Rows are
// plain elements with inline buttons; no row has its own menu, tooltip or hover card.

import { ArrowDownIcon, ArrowUpIcon, ChevronRightIcon, RefreshCwIcon, TriangleAlertIcon, XIcon } from "lucide-react";
import { useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
import type { App } from "../core/app";
import type { Prefs } from "../core/nav";
import { meterGroups, PAGES, setUpAll, untilText, USAGE_PROVIDERS, waitText, type AccountList, type Meter, type Page, type Profile, type ProviderInfo, type Runner, type SettingsState } from "../core/settings";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { store } from "./store";
import { chooseTheme, useTheme, type Theme } from "./theme";

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

// ---- meters ----

/** One window: its name, a bar of what is left, the percent and the time to its reset. */
function MeterLine({ meter, now }: { meter: Meter; now: number }) {
  const left = meter.used_pct === null ? null : 100 - meter.used_pct;
  if (left === null || meter.state === "missing" || meter.state === "failed")
    return (
      <div className="meter-line" data-testid="meter" data-state={meter.state}>
        <span className="meter-window">{meter.window ?? ""}</span>
        <span className="min-w-0 flex-1 truncate text-faint-foreground">{meter.reason ?? meter.state}</span>
      </div>
    );
  const reset = meter.reset_time_seconds === null ? null : meter.reset_time_seconds - now / 1000;
  const tone = left <= 5 ? "bg-failed" : left <= 20 ? "bg-waiting" : "bg-foreground/70";
  return (
    <div className="meter-line" data-testid="meter" data-state={meter.state} data-old={meter.state === "stale" || undefined}>
      <span className="meter-window">{meter.window ?? ""}</span>
      <span className="meter-bar" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={left} aria-label={`${meter.label} ${meter.window ?? ""} left`}>
        <span className={tone} style={{ width: `${left}%` }} />
      </span>
      <span className="w-8 text-right tabular-nums">{left}%</span>
      <span className="min-w-10 text-right whitespace-nowrap text-faint-foreground tabular-nums">{reset !== null && reset > 0 ? untilText(reset) : ""}</span>
    </div>
  );
}

function RefreshButton({ app, provider, now }: { app: App; provider: string; now: number }) {
  const refreshing = useSettings(app, "refreshing");
  const next = useSettings(app, "nextRefresh")[provider] ?? 0;
  const wait = next - now;
  const name = `Refresh ${label(provider)} usage`;
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      data-testid={`usage-refresh-${provider}`}
      aria-label={name}
      title={wait > 0 ? `Next refresh in ${waitText(wait)}` : name}
      disabled={refreshing !== null || wait > 0}
      onClick={() => void app.settings.refreshUsage(provider)}
    >
      <RefreshCwIcon className={refreshing === provider ? "animate-spin" : undefined} />
    </Button>
  );
}

/** When the next refresh of a provider is allowed, as text, or null when it is now. */
function useWait(app: App, provider: string, now: number) {
  const next = useSettings(app, "nextRefresh")[provider] ?? 0;
  return next > now ? waitText(next - now) : null;
}

const OPEN_KEY = "swarm.usage.open";

/** The sidebar footer: one line per account, and every window when open. */
export function UsageMeters({ app }: { app: App }) {
  const usage = useSettings(app, "usage");
  const nextRefresh = useSettings(app, "nextRefresh");
  const [open, setOpen] = useState(store.getItem(OPEN_KEY) === "1");
  const cooling = Object.values(nextRefresh).some((t) => t > Date.now());
  const now = useNow(cooling ? 1000 : 60_000);
  useEffect(() => {
    if (app.settings.state.usage.state === "idle") void app.settings.loadUsage();
  }, [app]);
  const groups = meterGroups(usage.state === "ready" ? usage.data : null);
  const toggle = () => {
    store.setItem(OPEN_KEY, open ? "0" : "1");
    setOpen(!open);
  };
  return (
    <section aria-label="Usage" data-testid="usage-meters" className="usage-meters">
      <div className="flex items-center gap-1">
        <button className="flex min-w-0 flex-1 items-center gap-1 text-left" aria-expanded={open} onClick={toggle}>
          <ChevronRightIcon className={`size-3 shrink-0 transition-transform ${open ? "rotate-90" : ""}`} />
          <span className="font-medium text-muted-foreground">Usage</span>
          {!open && <span className="min-w-0 flex-1 truncate text-faint-foreground">{groups.map((g) => `${g.label} ${summary(g.meters)}`).join(" · ")}</span>}
        </button>
        {usage.state === "error" && <TriangleAlertIcon className="size-3 text-failed" aria-label={usage.error} />}
      </div>
      {open &&
        USAGE_PROVIDERS.map((provider) => {
          const mine = groups.filter((g) => g.provider === provider);
          if (!mine.length) return null;
          return (
            <div key={provider} className="grid gap-0.5" data-testid={`usage-${provider}`}>
              <div className="flex items-center gap-1">
                <span className="min-w-0 flex-1 truncate text-muted-foreground">{label(provider)}</span>
                <Wait app={app} provider={provider} now={now} />
                <RefreshButton app={app} provider={provider} now={now} />
              </div>
              {mine.map((g) => (
                <div key={g.key} className="grid gap-0.5">
                  {(mine.length > 1 || g.label !== "default") && <span className="truncate pl-1 text-faint-foreground">{g.label}</span>}
                  {g.meters.map((m, i) => (
                    <MeterLine key={`${m.window}:${i}`} meter={m} now={now} />
                  ))}
                </div>
              ))}
            </div>
          );
        })}
    </section>
  );
}

function Wait({ app, provider, now }: { app: App; provider: string; now: number }) {
  const wait = useWait(app, provider, now);
  return wait ? (
    <span className="text-faint-foreground tabular-nums" data-testid={`usage-wait-${provider}`}>
      next {wait}
    </span>
  ) : null;
}

/** "87% left": the least left of an account's windows, or its reason. */
function summary(meters: Meter[]): string {
  const left = meters.filter((m) => m.used_pct !== null && (m.state === "fresh" || m.state === "stale")).map((m) => 100 - m.used_pct!);
  return left.length ? `${Math.min(...left)}%` : (meters[0]?.reason ?? meters[0]?.state ?? "");
}

// ---- dialog ----

export function SettingsDialog({ app }: { app: App }) {
  const open = useSettings(app, "open");
  const page = useSettings(app, "page");
  return (
    <Dialog open={open} onOpenChange={(o) => !o && app.settings.close()}>
      {open && (
        <DialogContent className="flex h-[min(600px,calc(100vh-104px))] w-[min(780px,calc(100vw-32px))] p-0" data-testid="settings" aria-describedby={undefined}>
          <nav aria-label="Settings pages" className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-border p-2">
            <DialogTitle className="px-2 pt-1 pb-2">Settings</DialogTitle>
            {PAGES.map(([id, title]) => (
              <button
                key={id}
                className={`rounded-md px-2 py-1.5 text-left text-sm ${page === id ? "bg-selected text-foreground" : "text-muted-foreground hover:bg-hover hover:text-foreground"}`}
                aria-current={page === id ? "page" : undefined}
                data-testid={`settings-page-${id}`}
                onClick={() => app.settings.show(id)}
              >
                {title}
              </button>
            ))}
          </nav>
          <div className="grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)] content-start gap-4 overflow-y-auto p-5" data-testid={`settings-${page}`}>
            <h2 className="text-lg font-semibold">{PAGES.find(([id]) => id === page)![1]}</h2>
            <Banner app={app} />
            <PageBody app={app} page={page} />
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}

function PageBody({ app, page }: { app: App; page: Page }) {
  if (page === "appearance") return <AppearancePage app={app} />;
  if (page === "notifications") return <NotificationsPage app={app} />;
  if (page === "profiles") return <ProfilesPage app={app} />;
  if (page === "setup") return <SetupPage app={app} />;
  return <AccountsPage app={app} />;
}

/** The refused write, the login note, or a destructive action waiting for its second press. */
function Banner({ app }: { app: App }) {
  const error = useSettings(app, "error");
  const login = useSettings(app, "login");
  const confirm = useSettings(app, "confirm");
  return (
    <>
      {error && (
        <p role="alert" data-testid="settings-error" className="flex items-start gap-1.5 text-sm break-words text-failed select-text">
          <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" />
          {error}
        </p>
      )}
      {login && (
        <p role="status" data-testid="login-note" className="rounded-md border border-border-strong px-3 py-2 text-sm">
          Finish signing in to {label(login.provider)} as “{login.account}” in the login pane <code className="font-mono">{login.pane}</code>. Then press Refresh accounts.
        </p>
      )}
      {confirm && (
        <div role="alertdialog" aria-label={confirm.action} data-testid="settings-confirm" className="grid gap-2 rounded-md border border-border-strong px-3 py-2 text-sm">
          <p>{confirm.text}</p>
          <div className="flex gap-2">
            <Button size="sm" data-testid="settings-confirm-yes" onClick={() => void app.settings.answer(true)}>
              {confirm.action}
            </Button>
            <Button size="sm" variant="outline" onClick={() => void app.settings.answer(false)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </>
  );
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

function Segmented<T extends string>({ label: name, value, options, onChange, testid }: { label: string; value: T; options: [T, string][]; onChange: (v: T) => void; testid: string }) {
  return (
    <div role="radiogroup" aria-label={name} className="view-switch m-0 w-56" data-testid={testid}>
      {options.map(([v, text]) => (
        <button key={v} role="radio" aria-checked={value === v} data-value={v} onClick={() => value !== v && onChange(v)}>
          {text}
        </button>
      ))}
    </div>
  );
}

/** `busy` ignores input but keeps focus, which `disabled` would drop to the page body. */
function Toggle({ label: name, checked, onChange, testid, id, disabled, busy }: { label: string; checked: boolean; onChange: (on: boolean) => void; testid: string; id?: string; disabled?: boolean; busy?: boolean }) {
  return <input type="checkbox" role="switch" className="toggle" id={id} disabled={disabled} aria-label={name} aria-busy={busy || undefined} aria-disabled={busy || undefined} data-testid={testid} checked={checked} onChange={(e) => !busy && onChange(e.target.checked)} />;
}

/** The owner's prefs from the last tree; a write shows once the tree after it arrives. */
function usePrefs(app: App): Prefs {
  return useSyncExternalStore(app.subscribe, () => app.view.tree?.prefs ?? NO_PREFS);
}
const NO_PREFS: Prefs = {};

function AppearancePage({ app }: { app: App }) {
  const theme = useTheme();
  const prefs = usePrefs(app);
  return (
    <div className="grid gap-4">
      <Row title="Theme">
        <Segmented<Theme>
          label="Theme"
          testid="pref-theme"
          value={theme}
          options={[
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ]}
          onChange={(t) => chooseTheme(app, t)}
        />
      </Row>
      <Row title="Send with" detail="The other key adds a new line.">
        <Segmented
          label="Send with"
          testid="pref-send-key"
          value={prefs.sendKey === "commandReturn" ? "commandReturn" : "return"}
          options={[
            ["return", "Return"],
            ["commandReturn", "⌘Return"],
          ]}
          onChange={(v) => void app.settings.setPref("sendKey", v)}
        />
      </Row>
    </div>
  );
}

const NOTICES: [keyof NonNullable<Prefs["notices"]> & string, string, string][] = [
  ["post", "Post notices", "A chat that waits for you posts a notice."],
  ["sound", "Play a sound", ""],
  ["done", "Notify when a chat is done", ""],
  ["badge", "Badge the Dock", "The Dock shows the number of chats that need input."],
];

function NotificationsPage({ app }: { app: App }) {
  const prefs = usePrefs(app);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const notices = prefs.notices ?? {};
  const muted = notices.mutedProjects ?? [];
  const projects = (tree?.sections ?? []).filter((s) => s.kind === "project" && s.path);
  return (
    <div className="grid gap-4">
      {NOTICES.map(([key, title, detail]) => (
        <Row key={key} title={title} detail={detail || undefined}>
          <Toggle label={title} testid={`notice-${key}`} checked={notices[key] !== false} onChange={(on) => void app.settings.setPref(`notices.${key}`, on)} />
        </Row>
      ))}
      <h3 className="mt-2 text-sm font-semibold">Mute projects</h3>
      {projects.length === 0 && <p className="text-sm text-muted-foreground">Import a project to set its mute choice.</p>}
      {projects.map((p) => (
        <Row key={p.path} title={p.title} detail={p.path!}>
          <Toggle label={`Mute ${p.title}`} testid="mute-project" checked={muted.includes(p.path!)} onChange={(on) => void app.settings.mute(p.path!, on)} />
        </Row>
      ))}
    </div>
  );
}

// ---- profiles ----

const runnerText = (r: Runner) => `${r.provider}/${r.model}/${r.effort}`;

type NameMode = { op: "new" } | { op: "rename" | "copy"; name: string };

function ProfilesPage({ app }: { app: App }) {
  const roles = useSettings(app, "roles");
  const busy = useSettings(app, "busy");
  const [naming, setNaming] = useState<NameMode | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  if (roles.state === "loading" || roles.state === "idle") return <p className="text-sm text-muted-foreground">Reading profiles…</p>;
  if (roles.state === "error") return <p className="text-sm text-failed select-text">Profiles unavailable: {roles.error}</p>;
  const data = roles.data;
  const edited = data.profiles.find((p) => p.name === editing) ?? null;
  return (
    <div className="grid min-w-0 gap-4">
      <MinUsage key={data.revision} app={app} value={data.min_usage_left_pct} />
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={busy} data-testid="profile-new" onClick={() => setNaming({ op: "new" })}>
          New profile…
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={busy}
          data-testid="profiles-reset"
          onClick={() => app.settings.ask("Reset every profile to the bundled list? Your profile changes are lost.", "Reset profiles", () => app.settings.profileOp({ op: "reset" }))}
        >
          Reset to bundled
        </Button>
      </div>
      {naming && <NameForm key={JSON.stringify(naming)} app={app} mode={naming} done={() => setNaming(null)} />}
      <ul className="grid min-w-0 gap-1" aria-label="Profiles" data-testid="profile-list">
        {data.profiles.map((p) => (
          <li key={p.name} className="grid min-w-0 gap-2 rounded-md border border-border px-3 py-2" data-testid="profile-row" data-name={p.name}>
            <div className="flex min-w-0 items-center gap-2">
              <div className="min-w-0 flex-1">
                <div className="font-mono text-sm">{p.name}</div>
                <div className="truncate text-xs text-muted-foreground">{p.runners.map(runnerText).join(" → ") || "No runners"}</div>
              </div>
              <Button size="sm" variant="ghost" disabled={busy} data-action="edit" onClick={() => setEditing(editing === p.name ? null : p.name)}>
                Edit
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} data-action="rename" onClick={() => setNaming({ op: "rename", name: p.name })}>
                Rename
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} data-action="copy" onClick={() => setNaming({ op: "copy", name: p.name })}>
                Copy
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                data-action="delete"
                onClick={() => app.settings.ask(`Delete the profile “${p.name}”?`, "Delete profile", () => app.settings.profileOp({ op: "delete", name: p.name }))}
              >
                Delete
              </Button>
            </div>
            {edited === p && <ProfileEditor key={data.revision} app={app} profile={p} done={() => setEditing(null)} />}
          </li>
        ))}
      </ul>
    </div>
  );
}

function MinUsage({ app, value }: { app: App; value: number }) {
  const busy = useSettings(app, "busy");
  const [text, setText] = useState(String(value));
  const pct = Number(text);
  const valid = text !== "" && Number.isInteger(pct) && pct >= 0 && pct <= 100;
  const save = () => valid && pct !== value && void app.settings.profileOp({ op: "min_usage", pct });
  return (
    <Row title="Usage left" detail="A runner is skipped when every account of its provider has less than this left.">
      <span className="flex items-center gap-1 text-sm">
        <input
          className={`${FIELD} w-16 text-right`}
          aria-label="Usage left minimum, percent"
          data-testid="min-usage"
          inputMode="numeric"
          value={text}
          disabled={busy}
          onChange={(e) => setText(e.target.value.replace(/\D/g, ""))}
          onBlur={save}
          onKeyDown={(e) => e.key === "Enter" && save()}
        />
        %
      </span>
    </Row>
  );
}

function NameForm({ app, mode, done }: { app: App; mode: NameMode; done: () => void }) {
  const busy = useSettings(app, "busy");
  const [name, setName] = useState(mode.op === "new" ? "" : mode.op === "copy" ? `${mode.name}.copy` : mode.name);
  const title = mode.op === "new" ? "New profile" : mode.op === "copy" ? `Copy “${mode.name}” as` : `Rename “${mode.name}” to`;
  const submit = async () => {
    const to = name.trim();
    if (!to) return;
    const ok = await app.settings.profileOp(mode.op === "new" ? { op: "new", name: to } : { op: mode.op, name: mode.name, to });
    if (ok) done();
  };
  return (
    <form
      className="flex items-end gap-2"
      data-testid="profile-name-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <label className="grid min-w-0 flex-1 gap-1 text-xs text-muted-foreground">
        {title}
        <input autoFocus className={`${FIELD} font-mono`} data-testid="profile-name" value={name} disabled={busy} onChange={(e) => setName(e.target.value)} placeholder="review.quick" />
      </label>
      <Button size="sm" type="submit" disabled={busy || !name.trim()}>
        Save
      </Button>
      <Button size="sm" type="button" variant="outline" onClick={done}>
        Cancel
      </Button>
    </form>
  );
}

/** The runners of one profile, tried in order. Sandbox, approval and permission are kept as they
 *  are unless the provider changes. */
function ProfileEditor({ app, profile, done }: { app: App; profile: Profile; done: () => void }) {
  const busy = useSettings(app, "busy");
  const providers = useSettings(app, "providers");
  const [runners, setRunners] = useState<Runner[]>(profile.runners);
  const info = (id: string): ProviderInfo | undefined => providers.find((p) => p.id === id);
  const change = (i: number, next: Runner) => setRunners(runners.map((r, j) => (j === i ? next : r)));
  const move = (i: number, by: number) => {
    const next = [...runners];
    [next[i], next[i + by]] = [next[i + by], next[i]];
    setRunners(next);
  };
  const save = async () => {
    if (await app.settings.profileOp({ op: "save", profile: { name: profile.name, runners } })) done();
  };
  return (
    <div className="grid gap-2 border-t border-border pt-2" data-testid="profile-editor">
      {runners.map((r, i) => {
        const efforts = info(r.provider)?.efforts ?? [r.effort];
        const extras = (["sandbox", "approval", "permission"] as const).filter((k) => r[k]).map((k) => `${k}=${r[k]}`);
        return (
          <div key={i} className="flex flex-wrap items-center gap-1.5" data-testid="runner">
            <span className="w-4 text-xs text-faint-foreground">{i + 1}</span>
            <select
              className={FIELD}
              aria-label={`Runner ${i + 1} provider`}
              data-testid="runner-provider"
              value={r.provider}
              onChange={(e) => {
                const p = info(e.target.value);
                change(i, { provider: e.target.value, model: "default", effort: p?.default_effort ?? "medium" });
              }}
            >
              {(providers.length ? providers : [{ id: r.provider, label: r.provider }]).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
            <input className={`${FIELD} w-40 font-mono`} aria-label={`Runner ${i + 1} model`} data-testid="runner-model" value={r.model} onChange={(e) => change(i, { ...r, model: e.target.value.trim() })} />
            <select className={FIELD} aria-label={`Runner ${i + 1} effort`} data-testid="runner-effort" value={r.effort} onChange={(e) => change(i, { ...r, effort: e.target.value })}>
              {(efforts.includes(r.effort) ? efforts : [r.effort, ...efforts]).map((e) => (
                <option key={e}>{e}</option>
              ))}
            </select>
            <Button size="icon-sm" variant="ghost" aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>
              <ArrowUpIcon />
            </Button>
            <Button size="icon-sm" variant="ghost" aria-label="Move down" disabled={i === runners.length - 1} onClick={() => move(i, 1)}>
              <ArrowDownIcon />
            </Button>
            <Button size="icon-sm" variant="ghost" aria-label="Remove runner" onClick={() => setRunners(runners.filter((_, j) => j !== i))}>
              <XIcon />
            </Button>
            {extras.length > 0 && <span className="font-mono text-xs text-faint-foreground">{extras.join(" ")}</span>}
          </div>
        );
      })}
      <div className="flex gap-2">
        <Button size="sm" variant="outline" data-testid="runner-add" onClick={() => setRunners([...runners, { provider: "claude", model: "default", effort: info("claude")?.default_effort ?? "medium" }])}>
          Add runner
        </Button>
        <span className="flex-1" />
        <Button size="sm" variant="outline" onClick={done}>
          Cancel
        </Button>
        <Button size="sm" data-testid="profile-save" disabled={busy || runners.some((r) => !r.model)} onClick={() => void save()}>
          Save
        </Button>
      </div>
    </div>
  );
}

// ---- accounts ----

const AUTH: Record<string, string> = { signed_in: "Signed in", signed_out: "Signed out", unavailable: "Sign-in unknown" };

// ---- setup & doctor ----

const STATE_TEXT = { ok: "OK", setup: "Needs setup", broken: "Needs you" } as const;

/** `swarm doctor`'s rows, each with its own Set up, and what Set up all runs. A group with a
 *  conflict needs the owner's own edit, so neither button runs it. The settings methods keep their
 *  errors in state, which the page shows, so a click does not wait for them. */
function SetupPage({ app }: { app: App }) {
  const doctor = useSettings(app, "doctor");
  const plan = useSettings(app, "plan");
  const busy = useSettings(app, "busy");
  const pending = useSettings(app, "pending");
  if (doctor.state === "error")
    return (
      <div className="grid gap-2">
        <p role="alert" className="text-sm break-words text-failed">
          Could not run swarm doctor: {doctor.error}
        </p>
        <div>
          <Button size="sm" variant="outline" data-testid="doctor-retry" onClick={() => void app.settings.loadSetup()}>
            Try again
          </Button>
        </div>
      </div>
    );
  if (doctor.state !== "ready")
    return (
      <p role="status" className="text-sm text-muted-foreground">
        Checking…
      </p>
    );
  const p = plan.state === "ready" ? plan.data : null;
  const all = p ? setUpAll(doctor.data, p) : null;
  const conflicted = new Set(p?.conflicts.map((c) => c.group));
  return (
    <div className="grid gap-4" data-testid="setup-page">
      <div className="flex items-center gap-2">
        <p className="flex-1 text-sm text-muted-foreground">What swarm sets up on this Mac.</p>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void app.settings.loadSetup()}>
          <RefreshCwIcon />
          Check again
        </Button>
        <Button size="sm" data-testid="setup-all" disabled={busy || !all || all.plan.length + all.own.length === 0} onClick={() => all && void app.settings.setUp(all.plan, all.own)}>
          Set up all
        </Button>
      </div>
      {busy && !pending && (
        <p role="status" className="text-sm text-muted-foreground">
          Working…
        </p>
      )}
      {pending && (
        <section className="grid gap-2 rounded-md border p-3" data-testid="setup-preview">
          <h3 className="text-sm font-semibold">Set up {[...pending.plan, ...pending.own].join(", ")}</h3>
          {pending.files.length > 0 && (
            <ul className="grid gap-1 font-mono text-xs break-all select-text" data-testid="setup-files">
              {pending.files.map((f, i) => (
                <li key={i}>{f.path}</li>
              ))}
            </ul>
          )}
          {pending.own.includes("plugin") && <p className="text-xs text-muted-foreground">Installs the swarm plugin in each agent CLI on PATH.</p>}
          {pending.own.includes("hud") && <p className="text-xs text-muted-foreground">Installs the usage HUD app.</p>}
          <div className="flex gap-2">
            <Button size="sm" data-testid="setup-apply" disabled={busy} autoFocus onClick={() => void app.settings.applySetUp()}>
              Apply
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => app.settings.cancelSetUp()}>
              Cancel
            </Button>
          </div>
        </section>
      )}
      {doctor.data.items.map((item) => (
        <div key={item.id} data-testid="doctor-item" data-id={item.id} data-state={item.state}>
          <Row title={item.title} detail={item.reason} live>
            <span className={`text-xs ${item.state === "ok" ? "text-muted-foreground" : item.state === "broken" ? "text-failed" : ""}`}>{STATE_TEXT[item.state]}</span>
            {item.state === "setup" && item.group && (
              <Button size="sm" variant="outline" data-testid="setup-item" disabled={busy || conflicted.has(item.group)} title={conflicted.has(item.group) ? "A conflict below needs your edit first" : undefined} onClick={() => void app.settings.setUpItem(item)}>
                Set up
              </Button>
            )}
          </Row>
        </div>
      ))}
      {plan.state === "error" && (
        <p role="alert" className="text-sm break-words text-failed">
          Could not plan setup: {plan.error}
        </p>
      )}
      {p && p.conflicts.length > 0 && (
        <section className="grid gap-2" data-testid="setup-conflicts">
          <h3 className="text-sm font-semibold">Conflicts</h3>
          {p.conflicts.map((c, i) => (
            <div key={i} className="text-sm">
              <div className="font-mono text-xs break-all">{c.file}</div>
              <div className="text-xs break-words text-muted-foreground">
                {c.entry}: {c.fix}
              </div>
            </div>
          ))}
        </section>
      )}
      {p && p.skipped.length > 0 && (
        <details data-testid="setup-skipped">
          <summary className="cursor-pointer text-sm">Skipped · {p.skipped.length}</summary>
          <ul className="mt-2 grid gap-1 text-xs break-words text-muted-foreground select-text">
            {p.skipped.map((s, i) => (
              <li key={i}>
                {s.group}: {s.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

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
              </div>
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
        className="flex items-end gap-2"
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
