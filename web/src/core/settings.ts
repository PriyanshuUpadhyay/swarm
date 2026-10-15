// Usage meters and the minimal Settings view (S5, Scope v1 items 10 and 11). Theme, send key and the
// notice switches are owner choices under `prefs` in choices.json, the file `swarm notices --due`
// and `/api/badge` read (ADR 0077). Profiles and accounts go to `swarm roles` and `swarm accounts`
// with the revision this page read, so a stale page gets a 409 and reloads. Framework-free.

import { ApiError } from "./api";
import type { App } from "./app";
import type { Load } from "./changes";
import type { Command } from "./commands";

export type Page = "appearance" | "notifications" | "profiles" | "accounts";
export const PAGES: [Page, string][] = [
  ["appearance", "Appearance"],
  ["notifications", "Notifications"],
  ["profiles", "Agent profiles"],
  ["accounts", "Accounts"],
];

/** One usage window of one account (`docs/usage-format.md`). */
export type Meter = {
  provider: string;
  account: string | null;
  label: string;
  window: string | null;
  window_minutes: number | null;
  used_pct: number | null;
  reset_time_seconds: number | null;
  state: "fresh" | "stale" | "missing" | "failed" | "no_source" | string;
  reason: string | null;
  reason_code: string | null;
  as_of_seconds: number | null;
};
export type Usage = { meters: Meter[] };
/** `swarm.hud.v1` from `/api/hud`. `app` is null when this swarm has no HUD app. */
export type Hud = { installed: boolean; running: boolean; app: string | null };
export type Runner = { provider: string; model: string; effort: string; sandbox?: string; approval?: string; permission?: string };
export type Profile = { name: string; runners: Runner[] };
export type Roles = { revision: string; min_usage_left_pct: number; profiles: Profile[] };
export type ProviderInfo = { id: string; label: string; efforts: string[]; default_effort: string; accounts: boolean };
export type Account = { name: string; email: string | null; auth_state: string; usage_state: string; remaining_pct: number | null; summary: string | null };
export type AccountList = { provider: string; state: string; revision: string; auto: string | null; accounts: Account[] };

export type ProfileOp =
  | { op: "save"; profile: Profile }
  | { op: "new"; name: string }
  | { op: "rename"; name: string; to: string }
  | { op: "copy"; name: string; to: string }
  | { op: "delete"; name: string }
  | { op: "reset" }
  | { op: "min_usage"; pct: number };

/** The providers with usage meters and accounts. AGY has neither. */
export const USAGE_PROVIDERS = ["claude", "codex"];
/** The server keeps one refresh per provider per 300 s (S2). */
export const REFRESH_SECONDS = 300;

export type SettingsState = {
  open: boolean;
  page: Page;
  usage: Load<Usage>;
  hud: Load<Hud>;
  /** A HUD install or removal in flight. */
  hudPending: boolean;
  /** When each provider may refresh again, in ms since 1970, from the server's `next_in_seconds`. */
  nextRefresh: Record<string, number>;
  refreshing: string | null;
  roles: Load<Roles>;
  providers: ProviderInfo[];
  accounts: Record<string, Load<AccountList>>;
  /** A profile or account write in flight. */
  busy: boolean;
  /** The last refused write or failed read, shown on the page. */
  error: string | null;
  /** The pane a login opened in, where the owner finishes signing in. */
  login: { provider: string; account: string; pane: string } | null;
  /** A destructive action waiting for a second press. */
  confirm: { text: string; action: string; run: () => Promise<unknown> } | null;
};

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));
const CONFLICT = "changed elsewhere. The page was reloaded; try again.";

/** "2h 5m", "3d 4h", "4m": the time to a reset, for a meter row. */
export function untilText(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.ceil((s % 3600) / 60);
  if (d) return h ? `${d}d ${h}h` : `${d}d`;
  if (h) return m && m < 60 ? `${h}h ${m}m` : `${h}h`;
  return `${Math.max(1, m)}m`;
}

/** "4:05": the wait before the next refresh. */
export const waitText = (ms: number) => {
  const s = Math.ceil(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** The first-run card shows once the tree is loaded: the HUD app exists, is not installed, and the owner never answered. */
export const hudCardVisible = (hud: Load<Hud>, treeLoaded: boolean, asked: boolean | undefined) => treeLoaded && hud.state === "ready" && hud.data.app !== null && !hud.data.installed && asked !== true;

/** The meters to show, one group per account. AGY and accounts with no usage source are left out. */
export function meterGroups(usage: Usage | null): { key: string; provider: string; label: string; meters: Meter[] }[] {
  const groups = new Map<string, { key: string; provider: string; label: string; meters: Meter[] }>();
  for (const m of usage?.meters ?? []) {
    if (!USAGE_PROVIDERS.includes(m.provider) || m.state === "no_source") continue;
    const key = `${m.provider}:${m.account ?? m.label}`;
    let g = groups.get(key);
    if (!g) groups.set(key, (g = { key, provider: m.provider, label: m.label, meters: [] }));
    g.meters.push(m);
  }
  return [...groups.values()];
}

export class Settings {
  state: SettingsState = {
    open: false,
    page: "appearance",
    usage: { state: "idle" },
    hud: { state: "idle" },
    hudPending: false,
    nextRefresh: {},
    refreshing: null,
    roles: { state: "idle" },
    providers: [],
    accounts: {},
    busy: false,
    error: null,
    login: null,
    confirm: null,
  };
  private listeners = new Set<() => void>();
  /** Counts HUD writes, so a read that began before a write cannot overwrite its reply. */
  private hudWrites = 0;

  constructor(private readonly app: App) {}

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private set(change: Partial<SettingsState>) {
    this.state = { ...this.state, ...change };
    for (const fn of this.listeners) fn();
  }

  private get api() {
    return this.app.api;
  }

  show(page: Page = this.state.page) {
    this.set({ open: true, page, error: null, confirm: null });
    if (page === "profiles") void this.loadRoles();
    if (page === "accounts") {
      for (const p of USAGE_PROVIDERS) void this.loadAccounts(p);
      void this.loadUsage();
      void this.loadHud();
    }
  }

  close() {
    this.set({ open: false, confirm: null, login: null });
  }

  // ---- usage ----

  /** The cached meters; this makes no provider call. */
  async loadUsage() {
    if (this.state.usage.state === "idle") this.set({ usage: { state: "loading" } });
    try {
      this.set({ usage: { state: "ready", data: await this.api.get<Usage>("/api/usage") } });
    } catch (err) {
      this.set({ usage: { state: "error", error: message(err) } });
    }
  }

  /** Asks the provider for new meters, once per 300 s. A press in the wait does nothing. */
  async refreshUsage(provider: string) {
    if (this.state.refreshing || Date.now() < (this.state.nextRefresh[provider] ?? 0)) return;
    this.set({ refreshing: provider });
    try {
      const r = await this.api.post<Usage & { refresh: { next_in_seconds: number } }>("/api/usage/refresh", { provider });
      // A refresh reply holds this provider's meters, a cooldown reply every provider's; only this
      // provider's are taken, so the others are not doubled.
      const others = this.state.usage.state === "ready" ? this.state.usage.data.meters.filter((m) => m.provider !== provider) : [];
      const mine = r.meters.filter((m) => m.provider === provider);
      this.set({ usage: { state: "ready", data: { meters: [...others, ...mine] } }, nextRefresh: { ...this.state.nextRefresh, [provider]: Date.now() + r.refresh.next_in_seconds * 1000 } });
    } catch (err) {
      // The server held the slot when it started the call, so the next one waits the full limit.
      this.set({ error: `Usage refresh failed: ${message(err)}`, nextRefresh: { ...this.state.nextRefresh, [provider]: Date.now() + REFRESH_SECONDS * 1000 } });
      this.app.notice(`Usage refresh failed: ${message(err)}`, true);
    } finally {
      this.set({ refreshing: null });
    }
  }

  // ---- usage HUD ----

  async loadHud() {
    const writes = this.hudWrites;
    try {
      const data = await this.api.get<Hud>("/api/hud");
      if (writes === this.hudWrites) this.set({ hud: { state: "ready", data } });
    } catch (err) {
      if (writes === this.hudWrites) this.set({ hud: { state: "error", error: message(err) } });
    }
  }

  /** Installs or removes the login item; the switch shows what the server reports. */
  async setHud(on: boolean) {
    this.hudWrites++;
    this.set({ hudPending: true });
    try {
      this.set({ hud: { state: "ready", data: await this.api.post<Hud>("/api/hud", { on }) } });
    } catch (err) {
      this.app.notice(`Usage HUD failed: ${message(err)}`, true);
      await this.loadHud();
    } finally {
      this.set({ hudPending: false });
    }
  }

  // ---- preferences ----

  /** One `prefs` key: `theme`, `sendKey`, `usageHUDAsked` or `notices.post|sound|done|badge`. */
  setPref(key: string, value: string | boolean): Promise<boolean> {
    return this.app.choose({ op: "pref", key, value });
  }

  mute(project: string, muted: boolean): Promise<boolean> {
    return this.app.choose({ op: "mute_project", project, muted });
  }

  // ---- profiles ----

  async loadRoles() {
    if (this.state.roles.state !== "ready") this.set({ roles: { state: "loading" } });
    try {
      const [roles, providers] = await Promise.all([
        this.api.get<Roles>("/api/roles"),
        this.state.providers.length ? null : this.api.get<{ providers: ProviderInfo[] }>("/api/providers").then((r) => r.providers),
      ]);
      this.set({ roles: { state: "ready", data: roles }, ...(providers ? { providers } : {}) });
    } catch (err) {
      this.set({ roles: { state: "error", error: message(err) } });
    }
  }

  /** One profile write on the revision this page read. True when it saved. */
  async profileOp(op: ProfileOp): Promise<boolean> {
    const roles = this.state.roles;
    if (roles.state !== "ready" || this.state.busy) return false;
    return this.write(
      () => this.api.post("/api/roles", { revision: roles.data.revision, ...op }),
      "Profiles",
      () => {
        this.app.life.forgetProfiles();
        return this.loadRoles();
      },
    );
  }

  // ---- accounts ----

  async loadAccounts(provider: string) {
    if (this.state.accounts[provider]?.state !== "ready") this.set({ accounts: { ...this.state.accounts, [provider]: { state: "loading" } } });
    let next: Load<AccountList>;
    try {
      next = { state: "ready", data: await this.api.get<AccountList>("/api/accounts", { provider }) };
    } catch (err) {
      next = { state: "error", error: message(err) };
    }
    this.set({ accounts: { ...this.state.accounts, [provider]: next } });
  }

  /** Opens the provider's own login in a pane that swarm starts; the browser never sees a
   *  credential. The account is registered first, so a failed pane still lists it. */
  async login(provider: string, name: string): Promise<boolean> {
    const list = this.state.accounts[provider];
    if (list?.state !== "ready" || this.state.busy) return false;
    let pane = "";
    const ok = await this.write(
      async () => {
        const r = await this.api.post<{ pane: string }>("/api/accounts/login", { provider, name, revision: list.data.revision, workspace: this.app.view.workspace ?? undefined });
        pane = r.pane;
      },
      "Accounts",
      () => this.reloadAccounts(),
    );
    if (ok) this.set({ login: { provider, account: name, pane } });
    return ok;
  }

  /** Clears swarm's account list. Provider homes, sign-ins and usage caches stay. */
  resetAccounts() {
    const list = USAGE_PROVIDERS.map((p) => this.state.accounts[p]).find((l) => l?.state === "ready");
    if (list?.state !== "ready") return;
    this.ask("Reset accounts to the provider defaults? Swarm forgets the accounts it added; their folders and sign-ins stay.", "Reset accounts", () =>
      this.write(() => this.api.post("/api/accounts/reset", { revision: list.data.revision }), "Accounts", () => this.reloadAccounts()),
    );
  }

  private reloadAccounts() {
    return Promise.all([...USAGE_PROVIDERS.map((p) => this.loadAccounts(p)), this.loadUsage()]);
  }

  // ---- shared ----

  ask(text: string, action: string, run: () => Promise<unknown>) {
    this.set({ confirm: { text, action, run } });
  }

  async answer(yes: boolean) {
    const c = this.state.confirm;
    this.set({ confirm: null });
    if (yes && c) await c.run();
  }

  /** Runs one owner write. A stale revision reloads the page and says so; any other refusal shows
   *  its text. Nothing retries, so a write never lands on a state the owner did not see. */
  private async write(run: () => Promise<unknown>, what: string, reload: () => Promise<unknown>): Promise<boolean> {
    this.set({ busy: true, error: null, login: null });
    try {
      await run();
      await reload();
      return true;
    } catch (err) {
      const stale = err instanceof ApiError && err.status === 409;
      this.set({ error: stale ? `${what} ${CONFLICT}` : message(err) });
      await reload();
      return false;
    } finally {
      this.set({ busy: false });
    }
  }

  commands(): Command[] {
    return [
      { id: "settings", title: "Settings…", chord: "Meta+,", run: () => this.show() },
      ...PAGES.map(([page, title]) => ({ id: `settings.${page}`, title: `Settings: ${title}`, run: () => this.show(page) })),
      ...USAGE_PROVIDERS.map((p) => ({ id: `usage.refresh.${p}`, title: `Refresh ${p === "claude" ? "Claude" : "Codex"} Usage`, run: () => void this.refreshUsage(p) })),
    ];
  }
}
