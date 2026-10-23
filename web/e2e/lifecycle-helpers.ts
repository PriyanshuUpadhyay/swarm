import { expect, type Page } from "@playwright/test";
import { join, resolve } from "node:path";

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
  tab.on("console", (m) => m.type() === "error" && !/\b(400|404|409)\b/.test(m.text()) && errors.push(m.text()));
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

/** Opens New Workspace from the project's "+", fills a name, and creates it. */
async function newWorkspace(tab: Page, project: string, fill: () => Promise<void>) {
  await header(tab, project).hover();
  await header(tab, project).locator('[data-action="new-workspace"]').click();
  const dialog = tab.getByTestId("new-workspace");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId("nw-folder")).not.toHaveValue("");
  await fill();
  await dialog.getByTestId("nw-create").click();
  return dialog;
}

export { server, api, readTree, open, header, workspace, shoot, newWorkspace };
