import { expect, type Locator, type Page } from "@playwright/test";
import { pierreUnloadNoise } from "./console-noise";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { TreeView } from "../src/core/nav";

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
  if (!process.env.E2E_SHOTS || browser !== "webkit") return;
  for (const theme of ["dark", "light"])
    await tab.evaluate((t) => (document.documentElement.dataset.theme = t), theme).then(() => tab.screenshot({ path: join(SHOTS, `s3-${name}-${theme}.png`) }));
  await tab.evaluate(() => delete document.documentElement.dataset.theme);
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

export async function clickTarget(button: Locator) {
  return button.evaluate((el) => {
    const { width, height } = el.getBoundingClientRect();
    const target = getComputedStyle(el, "::after");
    return {
      width: Math.max(width, Number.parseFloat(target.width) || 0),
      height: Math.max(height, Number.parseFloat(target.height) || 0),
    };
  });
}

export async function showOtherProjects(page: Page) {
  const row = page.locator('[data-kind="more-projects"]');
  if (await row.count() && (await row.getAttribute("aria-expanded")) === "false") await row.click();
}
