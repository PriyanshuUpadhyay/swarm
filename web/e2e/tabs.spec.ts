import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

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
    await tab.screenshot({ path: info.outputPath("tab-only-archive-close.png") });
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
});
