// Usage meters and the minimal Settings view (S5, Scope v1 items 10 and 11). Theme, send key and the
// notice switches are owner choices under `prefs` in choices.json, the file `swarm notices --due`
// and `/api/badge` read (ADR 0077). Profiles and accounts go to `swarm roles` and `swarm accounts`
// with the revision this page read, so a stale page gets a 409 and reloads. Framework-free.

import { ApiError } from "./api";
import type { App } from "./app";
import type { Load } from "./changes";
import type { Command } from "./commands";

export type Page = "appearance" | "notifications" | "profiles" | "accounts" | "setup";
export const PAGES: [Page, string][] = [
  ["appearance", "Appearance"],
  ["notifications", "Notifications"],
  ["profiles", "Agent profiles"],
  ["accounts", "Accounts"],
  ["setup", "Setup & doctor"],
];

/** One `swarm.doctor.v1` row. `group` names what sets it up: a `swarm setup` group, `plugin` or `hud`. */
export type DoctorItem = { id: string; title: string; state: "ok" | "setup" | "broken"; reason: string; group: string | null };
export type Doctor = { items: DoctorItem[] };
/** `swarm setup --plan --json`, cut to what the page shows. */
export type SetupPlan = {
  digest: string;
  files: { group: string; path: string }[];
  conflicts: { group: string; file: string; entry: string; fix: string }[];
  skipped: { group: string; reason: string }[];
};
/** The groups `swarm setup` applies from a plan's digest. */
export const PLAN_GROUPS = ["hooks", "trust", "herdr", "skills"];

/** What Set up all runs: the plan groups that need setup and have no conflict, then `plugin` and
 *  `hud` when they need it. A conflict needs the owner, so its group is left out. */
export function setUpAll(doctor: Doctor, plan: SetupPlan): { plan: string[]; own: string[] } {
  const needed = new Set(doctor.items.flatMap((i) => (i.state === "setup" && i.group ? [i.group] : [])));
  const conflicted = new Set(plan.conflicts.map((c) => c.group));
  return {
    plan: PLAN_GROUPS.filter((g) => needed.has(g) && !conflicted.has(g)),
    own: ["plugin", "hud"].filter((g) => needed.has(g)),
  };
}

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
  doctor: Load<Doctor>;
  plan: Load<SetupPlan>;
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

/**
 * The first-run card shows once the tree is loaded: the HUD app exists, is not installed, and the owner never answered.
 * While the HUD state is still unknown its space is held, so the sections below do not jump when it appears.
 */
export function hudCard(hud: Load<Hud>, treeLoaded: boolean, asked: boolean | undefined): "show" | "hold" | "none" {
  if (!treeLoaded || asked === true) return "none";
  if (hud.state === "idle" || hud.state === "loading") return "hold";
  return hud.state === "ready" && hud.data.app !== null && !hud.data.installed ? "show" : "none";
}

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
    doctor: { state: "idle" },
    plan: { state: "idle" },
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
    // loadSetup keeps its errors in state.doctor and state.plan, so nothing waits for it.
    if (page === "setup") void this.loadSetup();
    if (page === "accounts") {
      for (const p of USAGE_PROVIDERS) void this.loadAccounts(p);
      void this.loadUsage();
      // loadHud keeps its own error in state.hud, so nothing waits for it.
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
      // The Home card asks once, so the notice names where to try again.
      this.app.notice(`Usage HUD failed: ${message(err)}. Try again in Settings › Accounts.`, true);
      await this.loadHud();
    } finally {
      // A read that started during the write may carry the state before it, so drop its reply too.
      this.hudWrites++;
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

  // ---- setup & doctor ----

  async loadSetup() {
    const loading = { state: "loading" } as const;
    if (this.state.doctor.state !== "ready") this.set({ doctor: loading, plan: loading });
    const load = async <T>(path: string): Promise<Load<T>> => {
      try {
        return { state: "ready", data: await this.api.get<T>(path) };
      } catch (err) {
        return { state: "error", error: message(err) };
      }
    };
    const [doctor, plan] = await Promise.all([load<Doctor>("/api/doctor"), load<SetupPlan>("/api/setup/plan")]);
    this.set({ doctor, plan });
  }

  /** Sets up `plan` groups from a plan made just now, so the digest is the one the owner sees
   *  applied, then each of `own`. A changed file between the two reads is a 409. */
  setUp(plan: string[], own: string[] = []): Promise<boolean> {
    if (this.state.busy) return Promise.resolve(false);
    return this.write(
      async () => {
        try {
          if (plan.length) {
            const only = plan.join(",");
            const { digest } = await this.api.get<SetupPlan>("/api/setup/plan", { only });
            await this.api.post("/api/setup/apply", { only, digest });
          }
          for (const only of own) await this.api.post("/api/setup/apply", { only });
        } catch (err) {
          // Any failed swarm setup is a 409, so the CLI's own text names the cause, not "changed elsewhere".
          throw err instanceof ApiError ? new Error(err.message) : err;
        }
      },
      "Setup",
      () => this.loadSetup(),
    );
  }

  setUpItem(item: DoctorItem) {
    if (!item.group) return Promise.resolve(false);
    return PLAN_GROUPS.includes(item.group) ? this.setUp([item.group]) : this.setUp([], [item.group]);
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
