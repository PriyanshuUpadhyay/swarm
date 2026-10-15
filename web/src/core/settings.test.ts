import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import type { App } from "./app";
import { hudCard, meterGroups, Settings, setUpAll, untilText, waitText, type Doctor, type Meter, type SetupPlan } from "./settings";

const ROLES = { revision: "rev-1", min_usage_left_pct: 5, profiles: [{ name: "chat", runners: [] }] };

/** An App stand-in: `get` and `post` answer by path, and every call is recorded. */
function fakeApp(post: (path: string, body: Record<string, unknown>) => Promise<unknown>) {
  const calls: string[] = [];
  const app = {
    api: {
      get: async (path: string, query?: Record<string, string>) => {
        calls.push(`GET ${path}${query?.provider ? `?${query.provider}` : ""}${query?.only ? `?${query.only}` : ""}`);
        if (path === "/api/roles") return ROLES;
        if (path === "/api/doctor") return DOCTOR;
        if (path === "/api/setup/plan") return { ...PLAN, digest: `d-${query?.only ?? "all"}` };
        if (path === "/api/providers") return { providers: [] };
        if (path === "/api/accounts") return { provider: query!.provider, state: "ready", revision: "acc-1", auto: null, accounts: [] };
        return { meters: [meter("codex", "default", "primary", 50), meter("claude", "default", "5h", 90)] };
      },
      post: (path: string, body: Record<string, unknown>) => {
        calls.push(`POST ${path} ${JSON.stringify(body)}`);
        return post(path, body);
      },
    },
    view: { workspace: "/work/proj" },
    life: { forgetProfiles: () => calls.push("forget profiles") },
    notice: (text: string) => calls.push(`notice ${text}`),
    choose: async (op: unknown) => (calls.push(`choose ${JSON.stringify(op)}`), true),
  };
  return { settings: new Settings(app as unknown as App), calls };
}

const item = (id: string, state: "ok" | "setup" | "broken", group: string | null = id) => ({ id, title: id, state, reason: "", group });
const DOCTOR: Doctor = { items: [item("hooks", "setup"), item("trust", "setup"), item("skills", "setup"), item("herdr", "ok"), item("plugin", "setup"), item("hud", "ok"), item("notices", "broken", null)] };
const PLAN: SetupPlan = { digest: "d", files: [], conflicts: [{ group: "trust", file: "~/.claude.json", entry: "projects", fix: "remove it" }], skipped: [] };

describe("setup & doctor", () => {
  it("Set up all leaves out a conflicted group, ok rows and rows with no group", () => {
    expect(setUpAll(DOCTOR, PLAN)).toEqual({ plan: ["hooks", "skills"], own: ["plugin"] });
  });

  it("shows the plan's files first, then applies with that plan's digest, then the plugin, then reloads", async () => {
    const { settings, calls } = fakeApp(async () => ({ ok: true }));
    await settings.setUp(["hooks", "skills"], ["plugin"]);
    expect(calls).toEqual(["GET /api/setup/plan?hooks,skills"]);
    expect(settings.state.pending?.digest).toBe("d-hooks,skills");
    expect(await settings.applySetUp()).toBe(true);
    expect(settings.state.pending).toBeNull();
    expect(calls).toEqual([
      "GET /api/setup/plan?hooks,skills",
      'POST /api/setup/apply {"only":"hooks,skills","digest":"d-hooks,skills"}',
      'POST /api/setup/apply {"only":"plugin"}',
      "GET /api/doctor",
      "GET /api/setup/plan",
    ]);
    expect(settings.state.doctor).toEqual({ state: "ready", data: DOCTOR });
  });

  it("a second Set up while the first plan loads does nothing, so the newer preview cannot be replaced", async () => {
    const { settings, calls } = fakeApp(async () => ({ ok: true }));
    const first = settings.setUp(["hooks"]);
    await settings.setUp(["skills"]);
    await first;
    expect(calls).toEqual(["GET /api/setup/plan?hooks"]);
    expect(settings.state.pending?.plan).toEqual(["hooks"]);
    expect(settings.state.busy).toBe(false);
  });

  it("a failed apply shows the CLI's own text, since every failed swarm setup is a 409, and reloads", async () => {
    const { settings } = fakeApp(async () => {
      throw new ApiError(409, "swarm: none of claude, codex, agy or pi is on PATH");
    });
    await settings.setUp([], ["plugin"]);
    expect(await settings.applySetUp()).toBe(false);
    expect(settings.state.error).toBe("swarm: none of claude, codex, agy or pi is on PATH");
    expect(settings.state.plan.state).toBe("ready");
  });
});

const meter = (provider: string, account: string | null, window: string, used: number | null, state = "fresh"): Meter => ({
  provider,
  account,
  label: account ?? provider,
  window,
  window_minutes: null,
  used_pct: used,
  reset_time_seconds: null,
  state,
  reason: used === null ? "No cached usage" : null,
  reason_code: used === null ? "no_cached_usage" : null,
  as_of_seconds: null,
});

describe("meters", () => {
  it("groups windows by account and leaves out AGY and accounts with no source", () => {
    const groups = meterGroups({
      meters: [meter("claude", "default", "5h", 13), meter("claude", "default", "7d", 40), meter("codex", "work", "primary", null, "missing"), meter("codex", "key", "", null, "no_source"), meter("agy", null, "", null, "no_source")],
    });
    expect(groups.map((g) => [g.key, g.meters.length])).toEqual([
      ["claude:default", 2],
      ["codex:work", 1],
    ]);
  });

  it("writes reset and wait times short", () => {
    expect(untilText(3 * 86400 + 4 * 3600 + 5)).toBe("3d 4h");
    expect(untilText(2 * 3600 + 5 * 60)).toBe("2h 5m");
    expect(untilText(59)).toBe("1m");
    expect(waitText(245_000)).toBe("4:05");
  });
});

describe("usage refresh", () => {
  it("waits the server's next_in_seconds, and a press in the wait makes no call", async () => {
    const { settings, calls } = fakeApp(async () => ({ meters: [meter("codex", "default", "primary", 50), meter("claude", "default", "5h", 10)], refresh: { provider: "claude", state: "cooldown", next_in_seconds: 300 } }));
    await settings.loadUsage();
    await settings.refreshUsage("claude");
    await settings.refreshUsage("claude");
    expect(calls.filter((c) => c.startsWith("POST /api/usage/refresh"))).toHaveLength(1);
    expect(settings.state.nextRefresh.claude - Date.now()).toBeGreaterThan(299_000);
    expect(settings.state.usage).toMatchObject({ state: "ready", data: { meters: [{ provider: "codex", used_pct: 50 }, { provider: "claude", used_pct: 10 }] } });
    // A cooldown reply carries every provider; the codex meter is still there once.
    await settings.refreshUsage("codex");
    expect(settings.state.usage.state === "ready" && settings.state.usage.data.meters.map((m) => m.provider)).toEqual(["claude", "codex"]);
    await settings.refreshUsage("codex");
    expect(calls.filter((c) => c.startsWith("POST /api/usage/refresh"))).toHaveLength(2);
  });

  it("a failed refresh also waits, because the server held its slot", async () => {
    const { settings, calls } = fakeApp(async () => {
      throw new ApiError(409, "Claude credentials are unavailable");
    });
    await settings.refreshUsage("claude");
    await settings.refreshUsage("claude");
    expect(calls.filter((c) => c.startsWith("POST"))).toHaveLength(1);
    expect(settings.state.error).toBe("Usage refresh failed: Claude credentials are unavailable");
  });
});

describe("owner writes", () => {
  it("sends the revision it read, then reloads and forgets the new-chat menu's profiles", async () => {
    const { settings, calls } = fakeApp(async () => ({ revision: "rev-2" }));
    await settings.loadRoles();
    expect(await settings.profileOp({ op: "rename", name: "chat", to: "talk" })).toBe(true);
    expect(calls).toContain('POST /api/roles {"revision":"rev-1","op":"rename","name":"chat","to":"talk"}');
    expect(calls.slice(-3)).toEqual(["forget profiles", "GET /api/roles", "GET /api/providers"]);
    expect(settings.state.error).toBeNull();
  });

  it("a stale revision says the profiles changed elsewhere and reloads; another refusal shows its text", async () => {
    let reply: () => never = () => {
      throw new ApiError(409, "swarm: profiles changed on disk; reload and try again");
    };
    const { settings, calls } = fakeApp(async () => reply());
    await settings.loadRoles();
    const reads = calls.filter((c) => c === "GET /api/roles").length;
    expect(await settings.profileOp({ op: "reset" })).toBe(false);
    expect(settings.state.error).toBe("Profiles changed elsewhere. The page was reloaded; try again.");
    expect(calls.filter((c) => c === "GET /api/roles").length).toBe(reads + 1);
    reply = () => {
      throw new ApiError(400, "swarm: The first profile must be 'chat'.");
    };
    expect(await settings.profileOp({ op: "delete", name: "chat" })).toBe(false);
    expect(settings.state.error).toBe("swarm: The first profile must be 'chat'.");
  });

  it("opens a login pane with the account list's revision and the open workspace, then names the pane", async () => {
    const { settings, calls } = fakeApp(async () => ({ provider: "claude", account: "work", pane: "%7", state: "opened", revision: "acc-2" }));
    await settings.loadAccounts("claude");
    expect(await settings.login("claude", "work")).toBe(true);
    expect(calls).toContain('POST /api/accounts/login {"provider":"claude","name":"work","revision":"acc-1","workspace":"/work/proj"}');
    expect(settings.state.login).toEqual({ provider: "claude", account: "work", pane: "%7" });
  });

  it("resets accounts only after the second press", async () => {
    const { settings, calls } = fakeApp(async () => ({ revision: "acc-2" }));
    await settings.loadAccounts("codex");
    settings.resetAccounts();
    expect(calls.some((c) => c.startsWith("POST"))).toBe(false);
    await settings.answer(true);
    expect(calls).toContain('POST /api/accounts/reset {"revision":"acc-1"}');
  });

  it("shows the HUD card only when the tree is loaded, the app exists, is not installed, and was not asked", () => {
    const ready = (app: string | null, installed: boolean) => ({ state: "ready" as const, data: { installed, running: false, app } });
    expect(hudCard(ready("/a/SwarmHUD.app", false), true, undefined)).toBe("show");
    expect(hudCard(ready("/a/SwarmHUD.app", false), false, undefined)).toBe("none");
    expect(hudCard(ready(null, false), true, undefined)).toBe("none");
    expect(hudCard(ready("/a/SwarmHUD.app", true), true, undefined)).toBe("none");
    expect(hudCard(ready("/a/SwarmHUD.app", false), true, true)).toBe("none");
    expect(hudCard({ state: "error", error: "launchctl failed" }, true, undefined)).toBe("none");
  });

  it("holds the HUD card space while the HUD state is unknown, only for an owner who was never asked", () => {
    expect(hudCard({ state: "idle" }, true, undefined)).toBe("hold");
    expect(hudCard({ state: "loading" }, true, undefined)).toBe("hold");
    expect(hudCard({ state: "idle" }, true, true)).toBe("none");
    expect(hudCard({ state: "idle" }, false, undefined)).toBe("none");
  });

  it("setHud posts the switch, is pending meanwhile, and stores the reply", async () => {
    const hud = { installed: true, running: true, app: "/a/SwarmHUD.app" };
    let release: () => void = () => {};
    const { settings, calls } = fakeApp(() => new Promise((r) => (release = () => r(hud))));
    const p = settings.setHud(true);
    expect(settings.state.hudPending).toBe(true);
    release();
    await p;
    expect(calls).toEqual(['POST /api/hud {"on":true}']);
    expect(settings.state).toMatchObject({ hudPending: false, hud: { state: "ready", data: hud } });
  });

  it("a late loadHud reply does not overwrite a newer setHud reply", async () => {
    const hud = { installed: true, running: true, app: "/a/SwarmHUD.app" };
    const { settings } = fakeApp(async () => hud);
    let release: (v: unknown) => void = () => {};
    (settings as unknown as { app: { api: { get: unknown } } }).app.api.get = () => new Promise((r) => (release = r));
    const read = settings.loadHud();
    await settings.setHud(true);
    release({ installed: false, running: false, app: "/a/SwarmHUD.app" });
    await read;
    expect(settings.state.hud).toEqual({ state: "ready", data: hud });
  });

  it("a loadHud that starts while setHud is pending does not overwrite its reply", async () => {
    const hud = { installed: true, running: true, app: "/a/SwarmHUD.app" };
    let answer: (v: unknown) => void = () => {};
    const { settings } = fakeApp(() => new Promise((r) => (answer = r)));
    let release: (v: unknown) => void = () => {};
    (settings as unknown as { app: { api: { get: unknown } } }).app.api.get = () => new Promise((r) => (release = r));
    const write = settings.setHud(true);
    const read = settings.loadHud();
    answer(hud);
    await write;
    release({ installed: false, running: false, app: "/a/SwarmHUD.app" });
    await read;
    expect(settings.state.hud).toEqual({ state: "ready", data: hud });
  });

  it("a failed setHud reads the real state again and clears pending", async () => {
    const { settings, calls } = fakeApp(async () => {
      throw new ApiError(500, "launchctl failed");
    });
    await settings.setHud(true);
    expect(calls).toEqual(['POST /api/hud {"on":true}', "notice Usage HUD failed: launchctl failed. Try again in Settings › Accounts.", "GET /api/hud"]);
    expect(settings.state.hudPending).toBe(false);
  });

  it("notice switches and mutes are owner choices", async () => {
    const { settings, calls } = fakeApp(async () => null);
    await settings.setPref("notices.sound", false);
    await settings.mute("/work/proj", true);
    expect(calls).toEqual(['choose {"op":"pref","key":"notices.sound","value":false}', 'choose {"op":"mute_project","project":"/work/proj","muted":true}']);
  });
});
