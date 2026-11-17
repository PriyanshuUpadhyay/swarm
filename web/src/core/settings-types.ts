// The wire types, page list and limits of the Settings view, apart from its store in settings.ts.

export type Page = "appearance" | "notifications" | "profiles" | "accounts" | "keys" | "setup";
export const PAGES: [Page, string][] = [
  ["appearance", "Appearance"],
  ["notifications", "Notifications"],
  ["profiles", "Agent profiles"],
  ["accounts", "Accounts"],
  ["keys", "API keys"],
  ["setup", "Setup & doctor"],
];

/** A setup, notice or UI row from `swarm.doctor.v1`. Transcript reads use the built-in Rust worker. */
export type DoctorItem = { id: string; title: string; state: "ok" | "setup" | "broken"; reason: string; group: string | null };
export type Doctor = { items: DoctorItem[] };
/** `swarm setup <items> --plan --json`, cut to what the page shows. */
export type SetupPlan = {
  digest: string;
  files: { group: string; path: string }[];
  commands?: { group: string; line: string }[];
  conflicts: { group: string; file: string; entry: string; fix: string }[];
  skipped: { group: string; reason: string }[];
};

/** `swarm import conductor --json` (ADR 0106): the Conductor chats an import adds, and why it skips
 *  the others. */
export type ConductorPlan = {
  digest: string;
  chats: { conductorId: string; title: string; provider: string; cwd: string; open: boolean; messages: number; nativeLog: boolean }[];
  skipped: { conductorId: string; title: string; reason: string }[];
};


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

/** `swarm.keys.v1` from `/api/keys`: each API key the kit reads, and whether the Keychain holds
 *  it. No reply holds a key's value. */
export type ApiKey = { name: string; label: string; used_for: string; get_url: string; saved: boolean };
export type KeyList = { keys: ApiKey[] };

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
