import { expect, test, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { CLAUDE_EMAIL, LOGIN_PANE } from "./serve";

// Usage meters and Settings (S5). Both browsers share one server home and one usage refresh limit,
// so each case puts back what it changes, and the refresh case holds in either order.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;
const home = () => process.env.SWARM_E2E_HOME!;
const SHOTS = resolve(import.meta.dirname, "../../tmp/web-port");

async function api<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${server().origin}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { Authorization: `Bearer ${token()}`, "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`);
  return (await res.json()) as T;
}

type Roles = { revision: string; min_usage_left_pct: number; profiles: { name: string; runners: { provider: string; model: string; effort: string }[] }[] };

async function open(tab: Page) {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  // The stale cases expect a refused request, which the browser logs.
  tab.on("console", (m) => m.type() === "error" && !/\b(400|409)\b/.test(m.text()) && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

async function settings(tab: Page, page: "appearance" | "notifications" | "profiles" | "accounts") {
  await tab.keyboard.press("Meta+,");
  const dialog = tab.getByTestId("settings");
  await expect(dialog).toBeVisible();
  await dialog.getByTestId(`settings-page-${page}`).click();
  await expect(dialog.getByTestId(`settings-${page}`)).toBeVisible();
  return dialog;
}

/** Each theme of the open page, for the result, only when E2E_SHOTS is set and in Chromium. */
async function shoot(tab: Page, target: Locator | Page, name: string, browser: string) {
  if (!process.env.E2E_SHOTS || browser !== "chromium") return;
  for (const theme of ["dark", "light"]) {
    await tab.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
    await target.screenshot({ path: join(SHOTS, `s5-${name}-${theme}.png`) });
  }
  await tab.evaluate(() => delete document.documentElement.dataset.theme);
}

/** The CLI on the server's home, as the notice helper runs it. */
function swarm(args: string[], extra: Record<string, string> = {}): string {
  return execFileSync(process.env.SWARM_E2E_SWARM!, args, {
    env: { PATH: process.env.PATH, HOME: home(), SWARM_HOME: home(), SWARM_NOTIFY_APP: process.env.SWARM_E2E_NOTIFY_APP, ...extra },
    encoding: "utf8",
  });
}

/** Posts one notice, then reads what `swarm notices --due` would post. */
function due(chair = false): { sound: boolean }[] {
  swarm(["notify", "E2E notice"], chair ? { SWARM_SESSION_ID: process.env.SWARM_E2E_LIVE!, SWARM_AGENT_ID: "orchestrator" } : {});
  return JSON.parse(swarm(["notices", "--due"]));
}

const choices = () => JSON.parse(readFileSync(join(home(), ".swarm/choices.json"), "utf8")) as { prefs?: Record<string, unknown> & { notices?: Record<string, unknown> } };

test("usage meters show the cached reading, and a refresh waits for the server's 300 s limit", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const meters = tab.getByTestId("usage-meters");
  await expect(meters).toBeVisible();
  const toggle = meters.getByRole("button", { name: /Usage/ });
  if ((await toggle.getAttribute("aria-expanded")) === "false") await toggle.click();
  const claude = meters.getByTestId("usage-claude");
  await expect(meters.getByTestId("usage-codex")).toContainText("No cached usage");
  await claude.getByTestId("usage-refresh-claude").click();
  // The first browser refreshes; the second gets the cooldown with the reading the first took.
  await expect(claude.getByRole("meter", { name: "default 5h left" })).toHaveAttribute("aria-valuenow", "87");
  await expect(claude.getByRole("meter", { name: "default 7d left" })).toHaveAttribute("aria-valuenow", "58");
  await expect(claude.getByTestId("usage-refresh-claude")).toBeDisabled();
  await expect(claude.getByTestId("usage-wait-claude")).toHaveText(/^next [0-4]:\d\d$/);
  await expect(claude.getByTestId("usage-refresh-claude")).toHaveAttribute("title", /^Next refresh in [0-4]:\d\d$/);
  // The reply's other providers do not double the Codex meter.
  await expect(meters.getByTestId("usage-codex").getByTestId("meter")).toHaveCount(1);
  // A new page knows nothing of the wait; the server still holds it. The page took the token out
  // of its URL, so it opens the server's link again; a hash change alone would not load it.
  await tab.goto("about:blank");
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await meters.getByTestId("usage-refresh-claude").click();
  await expect(meters.getByTestId("usage-wait-claude")).toHaveText(/^next [0-4]:\d\d$/);
  const again = await api<{ refresh: { state: string; next_in_seconds: number } }>("/api/usage/refresh", { provider: "claude" });
  expect(again.refresh.state).toBe("cooldown");
  expect(again.refresh.next_in_seconds).toBeLessThanOrEqual(300);
  await shoot(tab, tab.locator(".bg-sidebar").first(), "meters", info.project.name);
  expect(errors).toEqual([]);
});

test("each notification switch is saved and read back by swarm notices --due", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const dialog = await settings(tab, "notifications");
  await shoot(tab, dialog, "notifications", info.project.name);
  const flip = async (key: string, on: boolean) => {
    const toggle = dialog.getByTestId(`notice-${key}`);
    if ((await toggle.isChecked()) !== on) await toggle.click();
    await expect(toggle).toBeChecked({ checked: on });
  };
  due(); // drain rows left by other cases
  expect(due()).toEqual([expect.objectContaining({ sound: true })]);
  await flip("sound", false);
  expect(choices().prefs?.notices?.sound).toBe(false);
  expect(due()).toEqual([expect.objectContaining({ sound: false })]);
  await flip("sound", true);
  // A row whose agent is not waiting is a done notice.
  await flip("done", false);
  expect(due()).toEqual([]);
  await flip("done", true);
  await flip("post", false);
  expect(due()).toEqual([]);
  await flip("post", true);
  expect(due()).toHaveLength(1);
  // A notice from the live chat is muted with its project.
  const mute = dialog.getByRole("switch", { name: "Mute ws-live" });
  await mute.click();
  await expect(mute).toBeChecked();
  expect(due(true)).toEqual([]);
  await mute.click();
  await expect(mute).not.toBeChecked();
  expect(due(true)).toHaveLength(1);
  // The badge is not a notice; the Dock reads /api/badge.
  await flip("badge", false);
  expect(choices().prefs?.notices?.badge).toBe(false);
  expect((await api<{ count: number }>("/api/badge")).count).toBe(0);
  await flip("badge", true);
  expect(choices().prefs?.notices).toMatchObject({ post: true, sound: true, done: true, badge: true, mutedProjects: [] });
  expect(errors).toEqual([]);
});

test("theme and send key are owner choices that the page follows", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const dialog = await settings(tab, "appearance");
  await shoot(tab, dialog, "appearance", info.project.name);
  await dialog.getByTestId("pref-theme").getByRole("radio", { name: "Light" }).click();
  await expect(tab.locator("html")).toHaveAttribute("data-theme", "light");
  await expect.poll(() => choices().prefs?.theme).toBe("light");
  await dialog.getByTestId("pref-send-key").getByRole("radio", { name: "⌘Return" }).click();
  await expect(dialog.getByTestId("pref-send-key").getByRole("radio", { name: "⌘Return" })).toHaveAttribute("aria-checked", "true");
  expect(choices().prefs?.sendKey).toBe("commandReturn");
  // Another window's choice reaches this page through the tree.
  const { revision } = await api<{ revision: string }>("/api/tree?view=workspaces");
  await api("/api/choices", { revision, op: "pref", key: "theme", value: "dark" });
  await expect(tab.locator("html")).toHaveAttribute("data-theme", "dark");
  await dialog.getByTestId("pref-theme").getByRole("radio", { name: "System" }).click();
  await dialog.getByTestId("pref-send-key").getByRole("radio", { name: "Return", exact: true }).click();
  await expect.poll(() => choices().prefs).toMatchObject({ theme: "system", sendKey: "return" });
  await expect(tab.locator("html")).not.toHaveAttribute("data-theme", /./);
  expect(errors).toEqual([]);
});

test("a profile edit saves through swarm roles, a stale page shows a conflict and reloads, and reset restores the bundled profiles", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const dialog = await settings(tab, "profiles");
  const row = (name: string) => dialog.locator(`[data-testid="profile-row"][data-name="${name}"]`);
  await expect(row("chat")).toBeVisible();
  await shoot(tab, dialog, "profiles", info.project.name);
  // Edit: the chat profile's first runner takes low effort.
  await row("chat").locator('[data-action="edit"]').click();
  const editor = row("chat").getByTestId("profile-editor");
  await editor.getByTestId("runner-effort").first().selectOption("low");
  await editor.getByTestId("profile-save").click();
  await expect(editor).toBeHidden();
  expect((await api<Roles>("/api/roles")).profiles[0].runners[0].effort).toBe("low");
  await expect(row("chat")).toContainText("/low");
  // Usage left minimum.
  await dialog.getByTestId("min-usage").fill("7");
  await dialog.getByTestId("min-usage").press("Enter");
  await expect.poll(async () => (await api<Roles>("/api/roles")).min_usage_left_pct).toBe(7);
  // Another writer changes the profiles under this page; its next write is refused and it reloads.
  const other = `other-${info.project.name}`;
  const mine = `mine-${info.project.name}`;
  await api("/api/roles", { revision: (await api<Roles>("/api/roles")).revision, op: "new", name: other });
  await dialog.getByTestId("profile-new").click();
  await dialog.getByTestId("profile-name").fill(mine);
  await dialog.getByTestId("profile-name").press("Enter");
  await expect(dialog.getByTestId("settings-error")).toHaveText("Profiles changed elsewhere. The page was reloaded; try again.");
  await expect(row(other)).toBeVisible();
  await expect(row(mine)).toHaveCount(0);
  await dialog.getByTestId("profile-name").press("Enter");
  await expect(row(mine)).toBeVisible();
  await expect(dialog.getByTestId("settings-error")).toHaveCount(0);
  // Reset asks once more, then restores the bundled list. It keeps the usage minimum.
  await dialog.getByTestId("min-usage").fill("5");
  await dialog.getByTestId("min-usage").press("Enter");
  await expect.poll(async () => (await api<Roles>("/api/roles")).min_usage_left_pct).toBe(5);
  await dialog.getByTestId("profiles-reset").click();
  await dialog.getByTestId("settings-confirm-yes").click();
  await expect(row(mine)).toHaveCount(0);
  const roles = await api<Roles>("/api/roles");
  expect(roles.profiles.map((p) => p.name)).not.toContain(other);
  expect(roles.profiles[0].runners[0].effort).toBe("high");
  expect(roles.min_usage_left_pct).toBe(5);
  expect(errors).toEqual([]);
});

test("accounts list each provider, and Open login starts the provider login in a pane that swarm opens", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const dialog = await settings(tab, "accounts");
  const claude = dialog.getByTestId("accounts-claude");
  const row = claude.locator('[data-testid="account-row"][data-name="default"]');
  await expect(row).toContainText(CLAUDE_EMAIL);
  await expect(row).toContainText("Signed in");
  await expect(row).toContainText("Auto");
  await expect(dialog.getByTestId("accounts-codex").locator('[data-testid="account-row"][data-name="default"]')).toContainText("Sign-in unknown");
  await shoot(tab, dialog, "accounts", info.project.name);
  const name = `e2e-${info.project.name}`;
  const typed = join(home(), "login-typed");
  const before = existsSync(typed) ? readFileSync(typed, "utf8") : "";
  await claude.getByTestId("account-name").fill(name);
  await claude.getByTestId("account-login").click();
  await expect(dialog.getByTestId("login-note")).toContainText(LOGIN_PANE);
  await expect(dialog.getByTestId("login-note")).toContainText(name);
  // The pane got the provider's own login with this account's folder; the page saw no credential.
  const line = readFileSync(typed, "utf8").slice(before.length);
  expect(line).toContain("'claude' 'auth' 'login'");
  expect(line).toContain(`accounts/claude/${name}`);
  await expect(claude.locator(`[data-testid="account-row"][data-name="${name}"]`)).toBeVisible();
  // A refused name shows the owner's text.
  await claude.getByTestId("account-name").fill("auto");
  await claude.getByTestId("account-login").click();
  await expect(dialog.getByTestId("settings-error")).toContainText("Invalid account name");
  // Reset forgets the added account and keeps the default one.
  await dialog.getByTestId("accounts-reset").click();
  await dialog.getByTestId("settings-confirm-yes").click();
  await expect(claude.locator(`[data-testid="account-row"][data-name="${name}"]`)).toHaveCount(0);
  await expect(row).toBeVisible();
  expect(errors).toEqual([]);
});

test("/setup opens Setup & doctor; a row's Set up applies its group and the row reads ok", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  await tab.goto(`${server().origin}/setup${server().hash}`);
  const dialog = tab.getByTestId("settings");
  const page = dialog.getByTestId("setup-page");
  await expect(page).toBeVisible();
  await expect(dialog.getByTestId("settings-page-setup")).toHaveAttribute("aria-current", "page");
  const ids = await page.getByTestId("doctor-item").evaluateAll((rows) => rows.map((r) => (r as HTMLElement).dataset.id));
  expect(ids.slice(0, 5)).toEqual(["hooks", "guard", "trust", "plugin", "hud"]);
  // The server home's hooks start unset, and the case removes them again at the end.
  const hooks = page.locator('[data-testid="doctor-item"][data-id="hooks"]');
  await expect(hooks).toHaveAttribute("data-state", "setup");
  try {
    await hooks.getByTestId("setup-item").click();
    // Nothing is written before Apply; the preview names the files.
    await expect(page.getByTestId("setup-files")).toContainText("hooks.json");
    expect(JSON.parse(swarm(["setup", "status", "--json"])).hooks).toBe(false);
    await page.getByTestId("setup-apply").click();
    await expect(hooks).toHaveAttribute("data-state", "ok");
    expect(new URL(tab.url()).pathname).toBe("/setup");
    expect(JSON.parse(swarm(["setup", "status", "--json"])).hooks).toBe(true);
  } finally {
    swarm(["managed", "revert", "--all"]);
  }
  expect(JSON.parse(swarm(["setup", "status", "--json"])).hooks).toBe(false);
  await tab.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  // The URL goes back to the open chat, or `/` with none.
  await expect.poll(() => new URL(tab.url()).pathname).not.toBe("/setup");
  expect(errors).toEqual([]);
});
