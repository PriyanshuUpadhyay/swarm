import { expect, type Page } from "@playwright/test";
import { pierreUnloadNoise } from "./console-noise";
import { join, resolve } from "node:path";
import type { Prefs, TreeView } from "../src/core/nav";

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;
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

type Tree = { revision: string; sections: { kind: string; path: string | null; title: string; rows: { id: string; kind: string; title: string; selection: { chat: string | null } }[] }[] };
const readTree = (view = "workspaces") => api<Tree>(`/api/tree?view=${view}`);

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

export { server, api, readTree, open, header, workspace, shoot, holdChatStart, compose };

export async function patchTreePrefs(tab: Page, patch: (prefs: Prefs) => void) {
  await tab.route("**/api/tree?*", async (route) => {
    const tree: TreeView = await (await route.fetch()).json();
    patch(tree.prefs);
    await route.fulfill({ json: tree });
  });
}

export async function shootThemes(tab: Page, prefix: string) {
  if (!process.env.E2E_SHOTS) return;
  for (const theme of ["dark", "light"])
    await tab.evaluate((t) => (document.documentElement.dataset.theme = t), theme).then(() => tab.screenshot({ path: `${prefix}-${theme}.png` }));
  await tab.evaluate(() => delete document.documentElement.dataset.theme);
}
