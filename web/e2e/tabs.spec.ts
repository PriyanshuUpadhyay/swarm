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
        await tab.close();
        writeFileSync(choicesPath, before);
      }
    }
  });
}

test("Rename chat prefills the title of a tab-only chat", async ({ page: tab }, info) => {
  const choicesPath = join(process.env.SWARM_E2E_HOME!, ".swarm/choices.json");
  const before = readFileSync(choicesPath, "utf8");
  try {
    await tab.goto(`${server().origin}/${server().hash}`);
    await tab.waitForFunction(() => window.__swarmReady === true);
    const workspace = tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" });
    const selected = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "selected_chat" && r.request().postDataJSON().chat !== null);
    await workspace.click();
    expect((await selected).status()).toBe(200);
    await expect(tab.getByTestId("tab")).toHaveCount(2);
    const workspaceId = (await workspace.getAttribute("data-id"))!;
    const session = (await tab.getByTestId("tab").nth(1).getAttribute("data-session"))!;
    await choose({ op: "open_tab", workspace: workspaceId, chat: session, tab_only: true });
    await choose({ op: "selected_chat", workspace: workspaceId, chat: session });
    const strip = await api<TabsView>(`/api/tabs?${new URLSearchParams({ workspace: workspaceId })}`);
    const title = strip.view.tabs.find((item) => item.session === session)!.title;
    expect(title).not.toBe("");
    await tab.reload();
    await tab.waitForFunction(() => window.__swarmReady === true);
    await expect(tab.locator(`[data-testid="tree-row"][data-session="${session}"]`)).toHaveCount(0);
    await tab.locator(`[data-testid="tab"][data-session="${session}"]`).click({ button: "right" });
    await tab.getByRole("menuitem", { name: "Rename chat…" }).click();
    await expect(tab.getByTestId("ask-name")).toHaveValue(title);
  } finally {
    try {
      await tab.screenshot({ path: info.outputPath("tab-only-rename.png") });
    } finally {
      await tab.close();
      writeFileSync(choicesPath, before);
    }
  }
});

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
    await choose({ op: "close_tab", workspace: workspaceId, chat: other });
    await choose({ op: "selected_chat", workspace: workspaceId, chat: session });
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
    await confirm.getByTestId("confirm-action").click();
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

test("closing the open tab-only chat shows the other tab after archive confirmation", async ({ page: tab }, info) => {
  const fixtureHome = process.env.SWARM_E2E_HOME!;
  const choicesPath = join(fixtureHome, ".swarm/choices.json");
  const before = readFileSync(choicesPath, "utf8");
  let restore: { session: string; pane: string } | undefined;
  try {
    await tab.goto(`${server().origin}/${server().hash}`);
    await tab.waitForFunction(() => window.__swarmReady === true);
    const workspace = tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" });
    const selected = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "selected_chat" && r.request().postDataJSON().chat !== null);
    await workspace.click();
    expect((await selected).status()).toBe(200);
    const tabs = tab.getByTestId("tab");
    await expect(tabs).toHaveCount(2);
    const workspaceId = (await workspace.getAttribute("data-id"))!;
    const session = (await tabs.nth(1).getAttribute("data-session"))!;
    const other = (await tabs.first().getAttribute("data-session"))!;
    const snapshot = await api<{ agents: Record<string, { id: string; pane: string }[]> }>("/api/snapshot");
    const pane = snapshot.agents[session].find((agent) => agent.id === "orchestrator")?.pane;
    expect(["fixture-tabs-1", "fixture-tabs-2"]).toContain(pane);
    restore = { session, pane: pane! };
    await choose({ op: "open_tab", workspace: workspaceId, chat: session, tab_only: true });
    await choose({ op: "selected_chat", workspace: workspaceId, chat: session });
    await tab.reload();
    await tab.waitForFunction(() => window.__swarmReady === true);
    const closing = tab.locator(`[data-testid="tab"][data-session="${session}"]`);
    await closing.click();
    await expect(closing).toHaveAttribute("aria-selected", "true");
    await expect(tab.locator(`[data-testid="tree-row"][data-session="${session}"]`)).toHaveCount(0);
    await closing.locator('[data-action="close"]').click();
    const confirm = tab.getByTestId("confirm");
    await expect(confirm).toBeVisible();
    await expect(confirm.getByTestId("confirm-action")).toHaveText("Archive Chat");
    await confirm.getByTestId("confirm-action").click();
    await expect(confirm).toBeHidden();
    await expect(closing).toHaveCount(0);
    await expect(tab.locator(`[data-testid="tab"][data-session="${other}"]`)).toHaveAttribute("aria-selected", "true");
    await expect(tab.getByTestId("workspace-landing")).toHaveCount(0);
  } finally {
    try {
      await tab.screenshot({ path: info.outputPath("tab-only-archive-close.png") });
    } finally {
      await tab.close();
      try {
        if (restore) {
          const { session, pane } = restore;
          await api("/api/sessions/unarchive", { ids: [session] });
          rmSync(join(fixtureHome, `closed-${pane}`), { force: true });
          // Closing clears the stored pane too; a chair rejoin restores it without a new chat.
          execFileSync(process.env.SWARM_E2E_SWARM!, ["agent", "add", "orchestrator", "orchestrator"], {
            env: {
              PATH: process.env.PATH, USER: "e2e", HOME: fixtureHome, SWARM_HOME: fixtureHome,
              CLAUDE_CONFIG_DIR: join(fixtureHome, ".claude"), CODEX_HOME: join(fixtureHome, ".codex"),
              SWARM_ADAPTER: "tmux-swarm", SWARM_SESSION_ID: session, FIXTURE_PANE: pane,
            },
            stdio: "pipe",
          });
        }
      } finally {
        writeFileSync(choicesPath, before);
      }
    }
  }
});
