import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Tab, TabsView, TreeView } from "../src/core/nav";

const server = () => new URL(process.env.SWARM_SERVE_URL!);

async function api<T>(path: string, body?: Record<string, unknown>): Promise<T> {
  const token = new URLSearchParams(server().hash.slice(1)).get("token")!;
  const response = await fetch(`${server().origin}${path}`, {
    method: body ? "POST" : "GET",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    ...(body && { body: JSON.stringify(body) }),
  });
  expect(response.status).toBe(200);
  return await response.json() as T;
}

async function choose(op: Record<string, unknown>) {
  const { revision } = await api<{ revision: string }>("/api/tree?view=workspaces");
  await api("/api/choices", { revision, ...op });
}

for (const pending of ["empty", null, "starting", "failed", "closing"] as const) {
  test(`workspace landing text reflects ${pending ?? "ready"} tabs`, async ({ page: tab }, info) => {
    const choicesPath = join(process.env.SWARM_E2E_HOME!, ".swarm/choices.json");
    const before = readFileSync(choicesPath, "utf8");
    try {
      await tab.route("**/api/tabs?*", async (route) => {
        const response = await route.fetch();
        const body = await response.json() as TabsView;
        body.view.selectedChat = null;
        body.view.tabs = pending === "empty" ? [] : body.view.tabs.map((item): Tab => ({ ...item, pending }));
        await route.fulfill({ response, json: body });
      });
      await tab.goto(`${server().origin}/${server().hash}`);
      await tab.waitForFunction(() => window.__swarmReady === true);
      await tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" }).click();
      const landing = tab.getByTestId("workspace-landing");
      await expect(landing).toBeVisible();
      await expect(landing).toContainText(pending === null ? "Pick a tab, or start a new chat." : "This workspace has no open chats.");
      await expect(landing.getByRole("button", { name: "New chat", exact: true })).toBeEnabled();
    } finally {
      try {
        await tab.screenshot({ path: info.outputPath("workspace-landing.png") });
      } finally {
        // The page polls the strip, so a read can be in the route when the case ends.
        await tab.unrouteAll({ behavior: "ignoreErrors" });
        await tab.close();
        writeFileSync(choicesPath, before);
      }
    }
  });
}

test("archiving the last open sidebar chat shows the workspace landing", async ({ page: tab }, info) => {
  const fixtureHome = process.env.SWARM_E2E_HOME!;
  const choicesPath = join(fixtureHome, ".swarm/choices.json");
  const before = readFileSync(choicesPath, "utf8");
  let restore: { session: string; pane: string } | undefined;
  let previousSelection: { workspace: string; chat: string | null } | undefined;
  try {
    await tab.goto(`${server().origin}/${server().hash}`);
    await tab.waitForFunction(() => window.__swarmReady === true);
    const workspace = tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" });
    const workspaceId = (await workspace.getAttribute("data-id"))!;
    const strip = await api<TabsView>(`/api/tabs?${new URLSearchParams({ workspace: workspaceId })}`);
    previousSelection = { workspace: workspaceId, chat: strip.view.selectedChat };
    const selected = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "selected_chat" && r.request().postDataJSON().chat !== null);
    await workspace.click();
    expect((await selected).status()).toBe(200);
    const tabs = tab.getByTestId("tab");
    await expect(tabs).toHaveCount(2);
    const session = (await tabs.first().getAttribute("data-session"))!;
    const other = (await tabs.nth(1).getAttribute("data-id"))!;
    const snapshot = await api<{ agents: Record<string, { id: string; pane: string }[]> }>("/api/snapshot");
    const pane = snapshot.agents[session].find((agent) => agent.id === "orchestrator")?.pane;
    expect(["fixture-tabs-1", "fixture-tabs-2"]).toContain(pane);
    restore = { session, pane: pane! };
    // Let the page save its selection and URL before reload, then close through its write queue.
    // An API-only selection leaves the old URL free to reopen the closed chat on reload.
    await tabs.first().click();
    await expect(tabs.first()).toHaveAttribute("aria-selected", "true");
    await tab.locator(`[data-testid="tab"][data-id="${other}"] [data-action="close"]`).click();
    await expect(tabs).toHaveCount(1);
    await tab.reload();
    await tab.waitForFunction(() => window.__swarmReady === true);
    await expect(tabs).toHaveCount(1);
    await expect(tabs.first()).toHaveAttribute("data-session", session);
    await expect(tabs.first()).toHaveAttribute("aria-selected", "true");
    const row = tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
    await row.click({ button: "right" });
    await tab.getByRole("menuitem", { name: "Archive chat", exact: true }).click();
    const confirm = tab.getByTestId("confirm");
    await expect(confirm).toBeVisible();
    await expect(confirm.getByTestId("confirm-action")).toHaveText("Archive Chat");
    // Enter takes the primary action, so the confirm opens with focus on it.
    await expect(confirm.getByTestId("confirm-action")).toBeFocused();
    await tab.keyboard.press("Enter");
    await expect(confirm).toBeHidden();
    await expect(tabs).toHaveCount(0);
    await expect(row).toHaveCount(0);
    await expect(tab.getByTestId("workspace-landing")).toBeVisible();
    await expect(tab.getByTestId("workspace-landing")).toContainText("This workspace has no open chats.");
  } finally {
    try {
      await tab.screenshot({ path: info.outputPath("last-sidebar-chat-archive.png") });
    } finally {
      await tab.close();
      try {
        if (restore) {
          const { session, pane } = restore;
          await api("/api/sessions/unarchive", { ids: [session] });
          rmSync(join(fixtureHome, `closed-${pane}`), { force: true });
          execFileSync(process.env.SWARM_E2E_SWARM!, ["agent", "add", "orchestrator", "orchestrator"], {
            env: {
              PATH: process.env.PATH, USER: "e2e", HOME: fixtureHome, SWARM_HOME: fixtureHome,
              CLAUDE_CONFIG_DIR: join(fixtureHome, ".claude"), CODEX_HOME: join(fixtureHome, ".codex"),
              SWARM_ADAPTER: "tmux-swarm", SWARM_SESSION_ID: session, FIXTURE_PANE: pane,
            },
            stdio: "pipe",
          });
        }
        if (previousSelection) {
          const { workspace, chat } = previousSelection;
          if (chat) await expect.poll(async () => {
            const tree = await api<TreeView>("/api/tree?view=workspaces");
            return tree.sections.some((section) => section.rows.some((row) => row.kind === "chat" && row.selection?.workspace === workspace && row.selection.chat === chat));
          }, { intervals: [100], timeout: 10_000 }).toBe(true);
          await choose({ op: "selected_chat", ...previousSelection });
        }
      } finally {
        writeFileSync(choicesPath, before);
      }
    }
  }
});

test("closing a tab keeps its chat in the sidebar and does not archive it", async ({ page: tab }, info) => {
  const choicesPath = join(process.env.SWARM_E2E_HOME!, ".swarm/choices.json");
  const before = readFileSync(choicesPath, "utf8");
  try {
    await tab.goto(`${server().origin}/${server().hash}`);
    await tab.waitForFunction(() => window.__swarmReady === true);
    const workspace = tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" });
    const selected = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "selected_chat" && r.request().postDataJSON().chat !== null);
    await workspace.click();
    expect((await selected).status()).toBe(200);
    const tabs = tab.getByTestId("tab");
    await expect(tabs).toHaveCount(2);
    const session = (await tabs.nth(1).getAttribute("data-session"))!;
    const other = (await tabs.first().getAttribute("data-session"))!;
    const closing = tab.locator(`[data-testid="tab"][data-session="${session}"]`);
    await closing.click();
    await expect(closing).toHaveAttribute("aria-selected", "true");
    await expect(tab.locator(`[data-testid="tree-row"][data-session="${session}"]`)).toHaveCount(1);
    await closing.locator('[data-action="close"]').click();
    await expect(closing).toHaveCount(0);
    await expect(tab.getByTestId("confirm")).toHaveCount(0);
    await expect(tab.locator(`[data-testid="tab"][data-session="${other}"]`)).toHaveAttribute("aria-selected", "true");
    await expect(tab.locator(`[data-testid="tree-row"][data-session="${session}"]`)).toHaveCount(1);
  } finally {
    try {
      await tab.screenshot({ path: info.outputPath("tab-close-keeps-chat.png") });
    } finally {
      await tab.close();
      writeFileSync(choicesPath, before);
    }
  }
});

test("a workspace name with line breaks draws on one line", async ({ page: tab }, info) => {
  const choicesPath = join(process.env.SWARM_E2E_HOME!, ".swarm/choices.json");
  const before = readFileSync(choicesPath, "utf8");
  try {
    await tab.goto(`${server().origin}/${server().hash}`);
    await tab.waitForFunction(() => window.__swarmReady === true);
    const rows = tab.locator('[data-testid="tree-row"][data-kind="workspace"]');
    const workspace = rows.filter({ hasText: "ws-tabs" });
    const workspaceId = (await workspace.getAttribute("data-id"))!;
    // WebKit breaks a nowrap line at U+2028, which the owner's saved name held.
    await choose({ op: "rename_workspace", workspace: workspaceId, name: "Fix the\u2028sidebar\nand tabs" });
    await tab.reload();
    await tab.waitForFunction(() => window.__swarmReady === true);
    const renamed = rows.filter({ hasText: "Fix the sidebar" });
    await expect(renamed).toHaveCount(1);
    // The text's own line boxes, because the row clips a second line.
    const drawnLines = (el: Element) => {
      const range = document.createRange();
      range.selectNodeContents(el);
      return new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
    };
    const label = renamed.locator(".label").first();
    await expect(label).toHaveText("Fix the sidebar and tabs");
    expect(await label.evaluate(drawnLines)).toBe(1);
    await renamed.click();
    const title = tab.locator(".tab-title");
    await expect(title).toHaveText("Fix the sidebar and tabs");
    expect(await title.evaluate(drawnLines)).toBe(1);
  } finally {
    try {
      await tab.screenshot({ path: info.outputPath("one-line-name.png") });
    } finally {
      await tab.close();
      writeFileSync(choicesPath, before);
    }
  }
});
