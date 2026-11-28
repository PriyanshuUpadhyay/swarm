import { expect, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { pierreUnloadNoise } from "./console-noise";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Agent, Snapshot } from "../src/core/api";
import type { Prefs, TreeView } from "../src/core/nav";
import type { ModelInfo } from "../src/core/run-catalog";
import type { Account, ProviderInfo } from "../src/core/settings-types";

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => {
  const value = new URLSearchParams(server().hash.slice(1)).get("token");
  if (!value) throw new Error("Missing fixture server token");
  return value;
};
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

const readTree = (view = "workspaces") => api<TreeView>(`/api/tree?view=${view}`);

export const fixtureSql = (statement: string) =>
  execFileSync("sqlite3", ["-cmd", ".timeout 5000", process.env.SWARM_E2E_DB!, statement]);

async function open(tab: Page) {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  // A refused request logs its response as a console error; the cases that expect one check the UI.
  tab.on("console", (m) => m.type() === "error" && !/\b(400|404|409)\b/.test(m.text()) && !pierreUnloadNoise(m) && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

const header = (tab: Page, project: string) => tab.locator('[data-testid="tree-row"][data-kind="project"]').filter({ hasText: project });
// The label, because a plain folder's row starts with the project's letter avatar.
const workspace = (tab: Page, title: string) => tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ has: tab.locator(".label", { hasText: new RegExp(`^${title}`) }) });

/** Screenshots for the result, in both themes, only when E2E_SHOTS is set and in WebKit. */
async function shoot(tab: Page, name: string, browser: string) {
  if (browser !== "webkit") return;
  await shootThemes(tab, join(SHOTS, `s3-${name}`));
}

/** Opens the composer on `project` from its "+" and types `prompt`. */
async function compose(tab: Page, project: string, prompt = "") {
  await header(tab, project).hover();
  await header(tab, project).locator('[data-action="new-chat-project"]').click();
  const composer = tab.getByTestId("new-chat-composer");
  await expect(composer.getByTestId("chip-project")).toHaveText(project);
  // The "+" opens a new composer, which takes the focus; the start page's own one may show first.
  await expect(composer.getByTestId("new-chat-prompt")).toBeFocused();
  if (prompt) await composer.getByTestId("new-chat-prompt").fill(prompt);
  return composer;
}

/** Answers `/api/chat/new` here and keeps each body, so no chat starts in the shared home. With no
 *  provider, `swarm launch` would wait about 10 s for a chair that never registers. */
async function holdChatStart(tab: Page) {
  const bodies: Record<string, unknown>[] = [];
  await tab.route("**/api/chat/new", async (route) => {
    bodies.push(route.request().postDataJSON());
    await route.fulfill({ json: { ok: true, start: `held-${bodies.length}` } });
  });
  return bodies;
}

export { server, token, api, readTree, open, header, workspace, shoot, holdChatStart, compose };

export async function patchTreePrefs(tab: Page, patch: (prefs: Prefs) => void) {
  await tab.route("**/api/tree?*", async (route) => {
    const tree: TreeView = await (await route.fetch()).json();
    patch(tree.prefs);
    await route.fulfill({ json: tree });
  });
}

export async function patchLiveChair(tab: Page, patch: (chair: Agent, snapshot: Snapshot, session: string) => void) {
  const session = process.env.SWARM_E2E_LIVE!;
  await tab.route("**/api/snapshot", async (route) => {
    const snapshot: Snapshot = await (await route.fetch()).json();
    patch(snapshot.agents[session][0], snapshot, session);
    await route.fulfill({ json: snapshot });
  });
}

export async function mockRunCatalog(tab: Page, { providers, models, accounts = [] }: {
  providers: (Pick<ProviderInfo, "id"> & Partial<ProviderInfo>)[];
  models: ModelInfo[] | ((provider: string | null) => ModelInfo[]);
  accounts?: (Pick<Account, "name" | "email"> & Partial<Account>)[];
}) {
  await tab.route("**/api/providers", (route) => route.fulfill({ json: { providers } }));
  await tab.route("**/api/models?*", (route) => {
    const provider = new URL(route.request().url()).searchParams.get("provider");
    return route.fulfill({ json: { models: typeof models === "function" ? models(provider) : models } });
  });
  await tab.route("**/api/accounts?*", (route) => route.fulfill({ json: { accounts } }));
}

export async function captureRunPost(tab: Page, action: "continue" | "resume", onPost?: () => void | Promise<void>) {
  const bodies: Record<string, unknown>[] = [];
  await tab.route(`**/api/chat/${action}`, async (route) => {
    bodies.push(route.request().postDataJSON());
    if (onPost) await onPost();
    await route.fulfill({ json: { session: process.env.SWARM_E2E_LIVE } });
  });
  return bodies;
}

export async function shootThemes(tab: Page, prefix: string) {
  if (!process.env.E2E_SHOTS) return;
  for (const theme of ["dark", "light"])
    await tab.evaluate((t) => (document.documentElement.dataset.theme = t), theme).then(() => tab.screenshot({ path: `${prefix}-${theme}.png` }));
  await tab.evaluate(() => delete document.documentElement.dataset.theme);
}

export function saveSidebarState() {
  const home = process.env.SWARM_E2E_HOME;
  if (!home) throw new Error("Missing the shared test home");
  const saved = ["choices.json", "view-state.json"].map((name) => {
    const path = join(home, ".swarm", name);
    return { path, before: existsSync(path) ? readFileSync(path) : null };
  });

  return () => {
    for (const { path, before } of saved) {
      if (before) writeFileSync(path, before);
      else rmSync(path, { force: true });
    }
  };
}

export async function clickTarget(control: Locator) {
  const { hit, ...size } = await control.evaluate((el) => {
    el.scrollIntoView({ block: "center", inline: "nearest" });
    const { x, y, width, height } = el.getBoundingClientRect();
    const target = getComputedStyle(el, "::after");
    const area = {
      width: Math.max(width, Number.parseFloat(target.width) || 0),
      height: Math.max(height, Number.parseFloat(target.height) || 0),
    };

    const extra = area.height - height;
    const above = document.elementFromPoint(x + width / 2, y - extra / 4);
    return { ...area, hit: extra > 0 && above === el };
  });
  expect(hit, "A point above the painted control must hit its expanded target").toBe(true);
  return size;
}

export async function showOtherProjects(page: Page) {
  const row = page.locator('[data-kind="more-projects"]');
  if (await row.count() && (await row.getAttribute("aria-expanded")) === "false") await row.click();
}
