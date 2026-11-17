// Shared helpers of the Settings e2e cases: the server, its home, and the Settings dialog.

import { expect, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

export const server = () => new URL(process.env.SWARM_SERVE_URL!);
export const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;
export const home = () => process.env.SWARM_E2E_HOME!;
export const SHOTS = resolve(import.meta.dirname, "../../tmp/web-port");

export async function api<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${server().origin}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { Authorization: `Bearer ${token()}`, "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`);
  return (await res.json()) as T;
}

export type Roles = { revision: string; min_usage_left_pct: number; profiles: { name: string; runners: { provider: string; model: string; effort: string }[] }[] };

export async function open(tab: Page) {
  const errors: string[] = [];
  // WebKit reports a fetch that a reload or goto cancels as "... due to access control checks".
  const expected = (m: string) => m.startsWith("ResizeObserver loop") || m.endsWith("due to access control checks.");
  tab.on("pageerror", (e) => !expected(e.message) && errors.push(e.message));
  // The stale cases expect a refused request, which the browser logs.
  tab.on("console", (m) => m.type() === "error" && !/\b(400|409)\b/.test(m.text()) && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

export async function settings(tab: Page, page: "appearance" | "notifications" | "profiles" | "accounts" | "keys") {
  await tab.keyboard.press("Meta+,");
  const dialog = tab.getByTestId("settings");
  await expect(dialog).toBeVisible();
  await dialog.getByTestId(`settings-page-${page}`).click();
  await expect(dialog.getByTestId(`settings-${page}`)).toBeVisible();
  return dialog;
}

/** Each theme of the open page, for the result, only when E2E_SHOTS is set and in WebKit. */
export async function shoot(tab: Page, target: Locator | Page, name: string, browser: string) {
  if (!process.env.E2E_SHOTS || browser !== "webkit") return;
  for (const theme of ["dark", "light"]) {
    await tab.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
    await target.screenshot({ path: join(SHOTS, `s5-${name}-${theme}.png`) });
  }
  await tab.evaluate(() => delete document.documentElement.dataset.theme);
}

/** The CLI on the server's home, as the notice helper runs it. */
export function swarm(args: string[], extra: Record<string, string> = {}): string {
  return execFileSync(process.env.SWARM_E2E_SWARM!, args, {
    env: { PATH: process.env.PATH, HOME: home(), SWARM_HOME: home(), SWARM_NOTIFY_APP: process.env.SWARM_E2E_NOTIFY_APP, ...extra },
    encoding: "utf8",
  });
}

/** Posts one notice, then reads what `swarm notices --due` would post. */
export function due(chair = false): { sound: boolean }[] {
  swarm(["notify", "E2E notice"], chair ? { SWARM_SESSION_ID: process.env.SWARM_E2E_LIVE!, SWARM_AGENT_ID: "orchestrator" } : {});
  return JSON.parse(swarm(["notices", "--due"]));
}

export const choices = () => JSON.parse(readFileSync(join(home(), ".swarm/choices.json"), "utf8")) as { prefs?: Record<string, unknown> & { notices?: Record<string, unknown> } };
