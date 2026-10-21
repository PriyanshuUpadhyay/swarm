import { expect, test } from "@playwright/test";

test("Meta+J shows and hides the background panel with the chat's agents", async ({ page: tab }, info) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env[`SWARM_E2E_PANES_${info.project.name}`]}"]`).click();
  await expect(tab.locator('[data-testid="transcript"][data-state="ready"]')).toBeVisible();

  const panel = tab.getByTestId("background-panel");
  await expect(panel).toHaveCount(0);
  await tab.keyboard.press("Meta+j");
  await expect(panel.getByRole("button", { name: /reviewer/ })).toBeVisible();
  await expect(panel.getByRole("button", { name: /tester/ })).toBeVisible();
  await tab.keyboard.press("Meta+j");
  await expect(panel).toHaveCount(0);
});
